//! 连 → 校验 host key → 鉴权。**全仓唯一与远端跑 SSH 传输层握手的地方**
//! （`tests/backend/dial_tests.rs::the_dial_only_happens_under_dial_home` 钉着「只在拨号核心」）。
//!
//! TCP 由这里自己拨（不交给 `client::connect`）：拨通之后问内核这一跳的往返时间
//! （[`crate::tcp_rtt`]），过**唯一一处**压缩判准 [`compression_for`]，再在这条 socket 上跑 SSH 握手。
//! 跳板可以多跳（`JumpHop::jump` 一层套一层，同 `ssh -J a,b`）：从最外一台起逐台经上一台的 direct-tcpip 开到下一台。

use copy_core::copy_text;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};

use crate::env::{self, Credential};
use crate::{DialRequest, Endpoint, JumpHop, Said, StageSink, Why};

/// 长连接的 SSH 层保活：**10 秒没收到对端字节就发一个**（russh 的 `keepalive_interval`：收到任何字节都重新计时）。
/// 发出去之后又过了 [`KEEPALIVE_MAX`] 轮还是一个字节都没收到 ⇒ russh 判死、断开这条连接（最后收包之后 `KEEPALIVE_MAX + 1` 轮，20 秒）。
/// **不靠 inactivity 拆链**：russh 的 inactivity 计时在无 keepalive 时会拆掉一条健康的空闲连接
/// （空闲一小时的会话很常见）。死链由 keepalive 超时 ＋ EOF 检出。
///
/// 为什么是 10 秒（从 30 秒改来）：路断成黑洞（换网 / NAT 表过期，没有 RST）之后，挂在这条连接上的开通道
/// 要等它被判死才失败、才换到新连接上 —— 30 秒 × 3 时是 120 秒，界面 45 秒的握手期限先到。
/// 代价：每条空闲连接每 10 秒一个保活往返（一个 `keepalive@openssh.com` 全局请求 ＋ 一条回话，各一包、百来字节）。
#[expect(
    clippy::disallowed_methods,
    reason = "拨号代理交给 `russh` 的 SSH 层 keepalive 间隔（`client::Config`）"
)]
const KEEPALIVE: Duration = Duration::from_millis(10_000);

/// 同一个间隔，毫秒（「多久没收包算久」照它定：`uses::STALE_AFTER_MS`）。
#[expect(
    clippy::cast_possible_truncation,
    reason = "常量 10 秒换成毫秒，远在 u64 之内"
)]
pub const KEEPALIVE_MS: u64 = KEEPALIVE.as_millis() as u64;

/// 保活发出去之后，再过几轮还没有任何回话就判死（russh 的 `keepalive_max`：计时到点时「已发未回」的个数超过它 ⇒ 断开）。
/// 1 ⇒ 最后收包之后第 1 轮发一个保活、第 2 轮到点时它还没回话 ⇒ 判死。一次保活要整整一轮（10 秒）没回话，健康的路上不会有。
pub const KEEPALIVE_MAX: usize = 1;

/// 短命探活（`probe`）的 inactivity 上限 —— 测试连接那条不需要保活。
#[expect(
    clippy::disallowed_methods,
    reason = "短命探活（测试连接）交给 russh 的 inactivity 上限：一条连接没动静多久算断，不是周期唤醒"
)]
const PROBE_INACTIVITY: Duration = Duration::from_millis(30_000);

/// 握手鉴权都过了的一条 SSH 连接。
pub struct Linked {
    pub session: client::Handle<Checker>,
    pub fingerprint: Option<String>,
    pub endpoint: String,
    /// 同一条胜者，结构化（ack 的 `winner`）。
    pub winner: Endpoint,
    /// 经跳板时跳板链那几条连接（外在先）：**必须与目标连接同生命周期**（drop 它 ⇒ 隧道死 ⇒ 目标断）。
    pub _jumps: Vec<client::Handle<Checker>>,
    /// 这条连接上的通道预算（`pool::Budget`）：长流 · 查询 · SFTP 同一条连接，同一道闸。
    pub budget: crate::pool::Budget,
    /// 这条连接托着的别的连接（主连接托着批量连接：长流在它就在，主连接没了随之释放 —— `pool.rs`）。
    pub held: Mutex<Vec<Arc<Linked>>>,
    /// 这条连接上停着的那一条空闲 sftp 会话（一个空位；`sftp.rs::Parked`）。不托本连接。
    pub idle_sftp: Mutex<Option<crate::sftp::Parked>>,
    /// 最后一次收到对端字节的时刻 ＋ 线上进出字节（[`Hearing`] 在 SSH 底下那条字节流上记）。
    pub heard: Arc<crate::pool::Heard>,
    /// 这一跳远不远（判准的答案）：SSH 层压没压是它过了闸（[`RUSSH_ZLIB_SOUND`]）；常驻流照它定要不要那台压。
    pub far: bool,
    /// 这一条的 TCP 经没经代理（出网口说的；经跳板时目标那条在跳板里头开 ⇒ 恒不经）。
    pub via_proxy: bool,
    /// 经跳板时各台跳板那一条（外在先，与 `_jumps` 一一对应）；跳板那几条不压（只运隧道，里面是已加密的字节）。
    pub jump_wires: Vec<JumpWire>,
}

/// 经过的一台跳板那一条：地址 · 记账 · 它的 TCP 经没经代理（只有最外那一台可能经）。
pub struct JumpWire {
    pub endpoint: String,
    pub heard: Arc<crate::pool::Heard>,
    pub via_proxy: bool,
}

/// 最后一个用它的人走了 ⇒ 这条连接收掉：给它所在的池记一笔「成员动了」（池存的是 `Weak`，看不见这一刻）。
impl Drop for Linked {
    fn drop(&mut self) {
        self.heard.tell_pool();
    }
}

impl Linked {
    /// 此刻的读数：这一条 ＋ 经过的各台跳板（[`crate::pool::LinkFacts`]）。
    pub fn facts(&self) -> crate::pool::LinkFacts {
        let wire = |endpoint: &str, far: bool, via_proxy: bool, h: &crate::pool::Heard| {
            let (bytes_in, bytes_out) = h.bytes();
            crate::pool::Wire {
                endpoint: endpoint.to_string(),
                far,
                via_proxy,
                bytes_in,
                bytes_out,
            }
        };
        crate::pool::LinkFacts {
            link: wire(&self.endpoint, self.far, self.via_proxy, &self.heard),
            jumps: self
                .jump_wires
                .iter()
                .map(|j| wire(&j.endpoint, false, j.via_proxy, &j.heard))
                .collect(),
        }
    }
}

/// SSH 底下那条字节流（直连的 TCP · 经跳板的那条隧道）套一层：**读到对端的字节**就记一笔收包时刻（[`crate::pool::Heard`]）。
/// 写不记。别的原样转手。
pub struct Hearing<S> {
    inner: S,
    heard: Arc<crate::pool::Heard>,
    /// 还没收到过对面的字节 ⇒ 收到第一批时报一步「收到 SSH 头」（[`crate::step::BANNER`]），之后不再报。
    first: Option<StageSink>,
}

impl<S> Hearing<S> {
    pub fn new(inner: S, heard: Arc<crate::pool::Heard>) -> Self {
        Hearing {
            inner,
            heard,
            first: None,
        }
    }

    /// 收到第一批字节时往 `stages` 报一步。
    pub fn telling(self, stages: &StageSink) -> Self {
        Hearing {
            first: Some(stages.clone()),
            ..self
        }
    }
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Hearing<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if matches!(r, std::task::Poll::Ready(Ok(()))) && buf.filled().len() > before {
            self.heard.touch();
            self.heard.took(buf.filled().len() - before);
            if let Some(s) = self.first.take() {
                s.step(crate::step::BANNER);
            }
        }
        r
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Hearing<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write(cx, buf);
        if let std::task::Poll::Ready(Ok(n)) = r {
            self.heard.gave(n);
        }
        r
    }
    fn poll_write_vectored(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let r = std::pin::Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        if let std::task::Poll::Ready(Ok(n)) = r {
            self.heard.gave(n);
        }
        r
    }
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    /// 关不上（经跳板的隧道：外面那条已经没了）也算关了：russh 收场时先关流、关失败就不报断开，这条连接会一直算活着。
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner)
            .poll_shutdown(cx)
            .map(|_| Ok(()))
    }
}

/// 一台的信任锚：库里这台记过的钥匙（[`crate::known_hosts`]）。
#[derive(Clone, Default)]
pub struct Trust {
    /// 这台记过的 host key（可多种类型）。非空 ⇒ 协商时这几种类型排前，握手时按协商出的类型严格比；
    /// 那一种没记过（而别的记过）⇒ 拒，同「钥匙变了」。
    pub known: Vec<PublicKey>,
}

impl Trust {
    /// 现读库里这台（`addrs` = 它的全部地址）。库读不出来 ⇒ 那一句（这一趟拒，不当首见）。
    pub fn of(addrs: &[Endpoint]) -> Result<Trust, Said> {
        let text = env::get().host_keys.read()?;
        Ok(Trust {
            known: crate::known_hosts::known(&text, addrs),
        })
    }

    /// 拒了这一把时「钥匙变了」那一句要说的期望指纹：库里同类型那一把（没有 ⇒ 空串）。
    fn expected_for(&self, offered: &PublicKey) -> String {
        self.known
            .iter()
            .find(|k| k.algorithm() == offered.algorithm())
            .map(crate::known_hosts::fingerprint)
            .unwrap_or_default()
    }

    /// 协商 host key 算法的偏好序：记过的那几种类型排前（按缺省序里的先后），其余照缺省序。没记过 ⇒ 缺省序。
    fn preferred_keys(&self) -> std::borrow::Cow<'static, [russh::keys::Algorithm]> {
        let default = russh::Preferred::DEFAULT.key;
        if self.known.is_empty() {
            return default;
        }
        let stored = |a: &russh::keys::Algorithm| {
            self.known
                .iter()
                .any(|k| key_type_of(a) == k.algorithm().as_str())
        };
        let mut first: Vec<russh::keys::Algorithm> =
            default.iter().filter(|a| stored(a)).cloned().collect();
        first.extend(default.iter().filter(|a| !stored(a)).cloned());
        std::borrow::Cow::Owned(first)
    }

    /// 按记过的钥匙判服务端给的这一把：`Ok(())` 过；`Err(())` 拒（同类型不是那一把 / 这一种没记过）。
    fn judge_known(&self, offered: &PublicKey) -> Result<(), ()> {
        let same_type: Vec<&PublicKey> = self
            .known
            .iter()
            .filter(|k| k.algorithm() == offered.algorithm())
            .collect();
        if same_type.iter().any(|k| k.key_data() == offered.key_data()) {
            Ok(())
        } else {
            Err(())
        }
    }
}

/// 协商用的签名算法 ⇒ 钥匙类型（RSA 的 `rsa-sha2-*` 都是 `ssh-rsa` 那一种钥匙）。
fn key_type_of(a: &russh::keys::Algorithm) -> String {
    match a {
        russh::keys::Algorithm::Rsa { .. } => "ssh-rsa".to_string(),
        other => other.as_str().to_string(),
    }
}

/// 握手时这一台给的那一把：指纹（报给界面）＋ OpenSSH 那一行的前两段（算法 ＋ base64；写不成那一形 ⇒ 无）。
#[derive(Clone)]
pub struct Offered {
    fingerprint: String,
    line: Option<String>,
}

/// 一条连接握手时那一把的格（校验那一刻写，接受与否都写）。
type Seen = Arc<Mutex<Option<Offered>>>;

fn fingerprint_in(seen: &Seen) -> Option<String> {
    seen.lock().ok()?.as_ref().map(|o| o.fingerprint.clone())
}

/// 鉴权过了：这台握手时给的那一把记进库（首见才追加；鉴权不过的对端写不进信任库）。写不了只记日志，不拦这条连接。
fn remember(addrs: &[Endpoint], seen: &Seen) {
    let Some(line) = seen.lock().ok().and_then(|g| g.as_ref()?.line.clone()) else {
        return;
    };
    if let Err(e) = env::get()
        .host_keys
        .rewrite(&mut |text| crate::known_hosts::remembered(text, addrs, &line))
    {
        tracing::warn!(
            "known_hosts：{} 没记下：{}",
            crate::known_hosts::names(addrs),
            e.logged()
        );
    }
}

/// host key 校验：库里记过就严格比，没记过（首见）接受并 `warn`。
/// **无论接受与否**都把实得那一把写回共享格：报给界面展示，鉴权过了按它记（[`remember`]）。
pub struct Checker {
    trust: Trust,
    observed: Seen,
    stages: StageSink,
    /// 握手的是哪一台 ＋ 阶段那一句里怎么称呼它。
    endpoint: Endpoint,
    label: String,
    /// 这条连接的收包记账；连接没了在这里记一笔（[`crate::pool::Heard::mark_gone`]）。
    heard: Arc<crate::pool::Heard>,
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn disconnected(
        &mut self,
        reason: client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        self.heard.mark_gone();
        match reason {
            client::DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            client::DisconnectReason::Error(e) => Err(e),
        }
    }

    async fn check_server_key(
        &mut self,
        offered: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // 本端不播报证书类 host key 算法（`russh::Preferred::host_key_certificates` 缺省为空）⇒ 对端交的是一把裸钥匙；
        // 交来证书就是协商之外的东西 ⇒ 不认（不拿证书里那把钥匙去比记过的，那等于绕过签发者校验）。
        let PublicKeyOrCertificate::PublicKey {
            key: server_public_key,
            ..
        } = offered
        else {
            return Ok(false);
        };
        let actual = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        let line = match server_public_key.to_openssh() {
            Ok(l) => Some(l.split_whitespace().take(2).collect::<Vec<_>>().join(" ")),
            Err(e) => {
                tracing::warn!("known_hosts：钥匙写不成 OpenSSH 形：{e}");
                None
            }
        };
        if let Ok(mut slot) = self.observed.lock() {
            *slot = Some(Offered {
                fingerprint: actual.clone(),
                line,
            });
        }
        // 到 host key 校验 = 该地址 TCP ＋ KEX 已过。
        self.stages.step(crate::step::HOSTKEY);
        self.stages.emit(crate::stage::host_key(
            self.endpoint.clone(),
            &self.label,
            &actual,
        ));
        // 协商出的这一把是什么类型（报错时带上：「钥匙变了」那一句要说是哪一种）。
        self.stages
            .note_key_type(server_public_key.algorithm().as_str());
        let pass = if !self.trust.known.is_empty() {
            self.trust.judge_known(server_public_key).is_ok()
        } else {
            tracing::warn!("dial: 首见的 host key {actual}：鉴权过了记进库");
            return Ok(true);
        };
        if pass {
            tracing::info!("dial: host key 与记过的那一把相同：{actual}");
        } else {
            tracing::error!(
                "dial: host key 对不上记过的（实得 {actual}，alg={}）—— 拒绝这条连接",
                server_public_key.algorithm()
            );
            self.stages
                .note_expected(&self.trust.expected_for(server_public_key));
        }
        Ok(pass)
    }
}

/// 把一次拨号失败粗分成阶段标签（界面泳道用不同图标）：按错误的**类型**分（`io::ErrorKind` / `russh::Error` 变体），
/// 不看错误串 —— 串是给人读的，改一个字不该改分类。认不出的一律 `other`。
pub fn stage_of_io(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::ConnectionRefused | K::HostUnreachable | K::NetworkUnreachable => "tcp",
        K::TimedOut => "timeout",
        _ => "other",
    }
}

/// 同上，russh 那一侧（握手期的错误；`IO` 那一支交给 [`stage_of_io`]）。
pub fn stage_of_russh(e: &russh::Error) -> &'static str {
    match e {
        russh::Error::IO(io) => stage_of_io(io),
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout
        | russh::Error::Elapsed(_) => "timeout",
        russh::Error::UnknownKey | russh::Error::WrongServerSig => "hostkey",
        _ => "other",
    }
}

/// 一个拨号地址给人看的那一串：`host:port`，v6 写成 `[host]:port`（阶段行 · 报错 · 手机编辑页预览同读这一处）。
pub fn label(ep: &Endpoint) -> String {
    if ep.host.contains(':') {
        format!("[{}]:{}", ep.host, ep.port)
    } else {
        format!("{}:{}", ep.host, ep.port)
    }
}

// ═══ **压缩：开不开，判准只此一处** ═══════════════════════════════════════════
//
// 用户要求：「智能多开链接\压缩等等」「这是属于 ssh 优化的部分」。SSH 传输层零压缩，
// 而会话数据 gzip 3.1–4.1×。russh 的默认偏好序（0.61 到 0.65 不变）是
// `[none, zlib, zlib@openssh.com]`，客户端按**自己的**偏好序挑双方都有的第一个 ⇒ **永远 `none`**。
//
// 判准看的是**这一跳真实的往返时间**（内核 `TCP_INFO`，`tcp_rtt`），不是地址长什么样：
// 用户的远端大半走覆盖网（地址在 RFC 1918 私网段、跨互联网、RTT 12–577 ms，实测）。

/// 往返时间到这个数（微秒）才压。回环 ≈ 0.07 ms、有线局域网 < 1 ms、无线局域网 1–5 ms；最近的一条跨互联网覆盖网 12 ms。
/// 判错两种代价不对称：局域网被判「远」⇒ 多花一点 zlib `fast` 档的 CPU；跨互联网被判「近」⇒ 白传 3–5 倍字节 ⇒ 门槛取低。
pub const COMPRESS_RTT_FLOOR_US: u32 = 5_000;

/// 压缩偏好序：**压**（`zlib@openssh.com` 鉴权之后才开 —— OpenSSH 服务端 `Compression yes` 今天只给它；
/// 远端 `Compression no` ⇒ 交集只剩 `none`，照样连得上）。
pub const COMPRESS_ON: &[russh::compression::Name] = &[
    russh::compression::ZLIB_LEGACY,
    russh::compression::ZLIB,
    russh::compression::NONE,
];

/// 压缩偏好序：**不压**。
pub const COMPRESS_OFF: &[russh::compression::Name] = &[russh::compression::NONE];

/// 🔴 **闸：russh 自己的 zlib 解压对不对**。**开了**（上游 russh 0.62.7 起修好了解压）。
///
/// 来历（NT1 现打）：上游 `russh 0.61.1` / `0.61.2` 的 `Decompress::decompress` 收尾判断用的是**这一次调用之前**的进度
/// （`n_in_` / `n_out_`）⇒ 解出来的字节比输入多一倍以上时，它在输出缓冲第二次撑满的那一刻提前收工：一包只交出 ≈ 2 × 包长，
/// 余下的留在解压器里、拼进**下一包** ⇒ 包流错位。真 sshd（`OpenSSH_10.2p1`，`zlib@openssh.com`）上的样子：鉴权过了、
/// 第一条通道的确认被吃掉、会话卡死。
///
/// 上游 issue #749 修在 `0.62.7`；压那一半同病（不可压的满长包只交出前一截 ⇒ 对端断链，WIN3 读数 A），`0.65` 也修了。
/// 今天链 crates.io 的 `0.65`（`dial_compress_tests::russh_comes_from_crates_io_at_a_version_with_the_zlib_fix` 钉来源与版本）
/// ⇒ 闸开：判准的答案落到连接上。
/// `dial_compress_tests::the_gate_matches_what_russh_really_does` 两向钉着「闸 == russh 一来一回对不对」：
/// 谁换回一份还坏着的 russh 而闸还开着 ⇒ 红；russh 对着而闸被关回去 ⇒ 红。
pub const RUSSH_ZLIB_SOUND: bool = true;

/// 🔴 **判准只此一处**：这一跳（`peer` ＋ 内核量到的往返微秒）要不要开 SSH 压缩。
///
/// - 回环 ⇒ 不压（压了只花 CPU）；
/// - 往返时间读得到 ⇒ `≥ COMPRESS_RTT_FLOOR_US` 才压；
/// - 读不到（平台答不上来）⇒ **压**（宁可在局域网上多花一点 CPU，也不在跨互联网那一跳上白传字节）。
pub fn compression_for(peer: IpAddr, rtt_us: Option<u32>) -> bool {
    if peer.to_canonical().is_loopback() {
        return false;
    }
    rtt_us.is_none_or(|us| us >= COMPRESS_RTT_FLOOR_US)
}

/// 生产用的 Config：保活照 [`KEEPALIVE`]，host key 算法的偏好序按这一台的信任锚排（记过的类型排前，[`Trust::preferred_keys`]）。
fn config_for(probe: bool, compress: bool, trust: &Trust) -> Arc<client::Config> {
    let mut c = Arc::try_unwrap(config_with(probe, compress, KEEPALIVE))
        .unwrap_or_else(|_| unreachable!("刚建出来的 Config 只有这一个引用"));
    c.preferred.key = trust.preferred_keys();
    Arc::new(c)
}

/// 保活间隔由调用方给（生产只走 [`config_for`]；判据把间隔按比例缩小，判死门槛 [`KEEPALIVE_MAX`] 照生产）。
pub fn config_with(probe: bool, compress: bool, keepalive: Duration) -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: probe.then_some(PROBE_INACTIVITY),
        keepalive_interval: (!probe).then_some(keepalive),
        keepalive_max: KEEPALIVE_MAX,
        preferred: russh::Preferred {
            compression: std::borrow::Cow::Borrowed(if compress {
                COMPRESS_ON
            } else {
                COMPRESS_OFF
            }),
            ..russh::Preferred::DEFAULT
        },
        ..Default::default()
    })
}

/// 经代理那一跳的握手期限（代理收了连接却一直不答 ⇒ 这么久之后那一下读报错返回，拨号报 `timeout`）：一次阻塞读写的上限，不是周期唤醒。
#[expect(
    clippy::disallowed_methods,
    reason = "经代理拨 SSH 时代理握手那几次阻塞读写的上限（出网口装在 socket 上），不是会自己醒来的构件"
)]
const PROXY_HANDSHAKE: Duration = Duration::from_millis(30_000);

/// 拨 TCP，拨通后问内核这一跳的往返时间、过一遍判准。回 `(socket, 这一跳远不远（判准的答案）, 经没经代理)` ——
/// SSH 层压不压是这个答案过了闸（[`RUSSH_ZLIB_SOUND`]），由开 SSH 的那一处现算；答案本身进连接的读数（常驻流照它定要不要那台压）。
/// 经没经代理是出网口开它那一刻说的，照原样带出去（进连接的读数）。
///
/// TCP 经出网口开（`out`：生产是这台的那一份 `egress::here()`）：直连，或经代理（握手完了交回的就是到那台的隧道）。
/// 跳板那一形不走这里的是目标那一跳（它走跳板的 direct-tcpip）；最外那一台跳板的 TCP 照样经这里。
async fn tcp_hop(
    ep: &Endpoint,
    out: &egress::Egress,
) -> Result<(tokio::net::TcpStream, bool, bool), (&'static str, std::io::Error)> {
    // 解析不出来与拨不通是两件事，原因码分开（出网口按步交回）；「测试档不许出网」那一句断言住出网口真开 TCP 的那一处。
    let (host, port, out) = (ep.host.clone(), ep.port, out.clone());
    let opened =
        tokio::task::spawn_blocking(move || egress::open(&out, &host, port, Some(PROXY_HANDSHAKE)))
            .await
            .map_err(|e| ("other", std::io::Error::other(e)))?
            .map_err(|f| {
                match f.stage {
                    // 解析出来零个地址：那一句照旧。
                    egress::Stage::Resolve if f.io.kind() == std::io::ErrorKind::NotFound => (
                        "resolve",
                        std::io::Error::other(copy_text("beConnect.race.noAddress", &[])),
                    ),
                    egress::Stage::Resolve => ("resolve", f.io),
                    egress::Stage::Connect | egress::Stage::Proxy => (stage_of_io(&f.io), f.io),
                }
            })?;
    let via_proxy = opened.via_proxy;
    let std_tcp = opened.tcp;
    // 期限只管代理握手那几下；之后这条 socket 交给异步运行时（非阻塞，期限也不再生效），照旧清掉。
    let to_async = |s: std::net::TcpStream| -> std::io::Result<tokio::net::TcpStream> {
        s.set_read_timeout(None)?;
        s.set_write_timeout(None)?;
        s.set_nonblocking(true)?;
        tokio::net::TcpStream::from_std(s)
    };
    let tcp = to_async(std_tcp).map_err(|e| (stage_of_io(&e), e))?;
    let rtt = crate::tcp_rtt::rtt_us(&tcp);
    // 经代理 ⇒ 这条 socket 的对端是代理：量到的往返不是到那台的 ⇒ 照「读不到」那一格（压）。
    let far =
        via_proxy || compression_for(tcp.peer_addr().map_err(|e| (stage_of_io(&e), e))?.ip(), rtt);
    tracing::info!(
        "dial: {} 往返 {} ⇒ 判准：{}{}",
        label(ep),
        rtt.map_or_else(|| "读不到".to_string(), |us| format!("{us} µs")),
        if far { "压" } else { "不压" },
        if far && !RUSSH_ZLIB_SOUND {
            "（russh 的 zlib 解压有缺陷，闸关着 ⇒ 这一条 SSH 层不压）"
        } else {
            ""
        }
    );
    Ok((tcp, far, via_proxy))
}

/// 竞速的胜者：句柄 · 指纹格 · 胜出的那个地址 · 这一跳远不远（判准的答案）· 它的 TCP 经没经代理 · 它那条字节流的记账。
struct Won {
    session: client::Handle<Checker>,
    seen: Seen,
    winner: Endpoint,
    far: bool,
    via_proxy: bool,
    heard: Arc<crate::pool::Heard>,
}

/// 多地址竞速：**同时**对每个地址起拨（TCP ＋ 握手 ＋ host key 校验，各自独立的校验器与指纹格），
/// 首个握手成功者胜、其余立即丢弃（drop 关 socket）。鉴权留给调用方只对胜者做一次
/// （防 agent 并发撞 `MaxAuthTries`）。全失败 ⇒ 聚合各地址的错误；被丢弃的输家不算错误。
///
/// TCP 由这里自己拨（不再交给 `client::connect`）：拨通之后问内核这一跳的往返时间、过压缩判准
/// （[`compression_for`]），再在这条 socket 上跑 SSH 握手。`apply = false` ⇒ 判准照问、答案**不用在这一条上**
/// 而是回给调用方（跳板那一形：跳板自己只运隧道、里面是已加密的字节，压不动；该压的是隧道里的目标那条）。
/// 回胜者（[`Won`]）。TCP 经 `out` 那一份出网描述开。
#[expect(
    clippy::result_large_err,
    reason = "失败带着给人看的整句（`Said`）原样往上交，一次拨号只走一次；手机那一侧共用这一形"
)]
async fn race(
    probe: bool,
    apply: bool,
    trust: Trust,
    order: Vec<Endpoint>,
    stages: &StageSink,
    out: &egress::Egress,
) -> Result<Won, (Said, Option<String>)> {
    use tokio::task::JoinSet;
    let addr_list = order.iter().map(label).collect::<Vec<_>>().join(", ");
    // 任一地址看到过的指纹（失败时也回报给界面，TOFU 展示要它）。
    let seen: Arc<Mutex<Option<String>>> = Arc::default();
    let seen_for_race = Arc::clone(&seen);
    let race = async move {
        // 每个地址那一格没成 ⇒ 码是它停在的阶段（`resolve` · `tcp` · `hostkey` …），句子带那条地址。
        let mut set: JoinSet<Result<_, Said>> = JoinSet::new();
        for ep in order {
            let stages = stages.clone();
            let observed: Seen = Arc::default();
            let trust = trust.clone();
            let seen = Arc::clone(&seen_for_race);
            let out = out.clone();
            set.spawn(async move {
                let ep_label = label(&ep);
                stages.emit(crate::stage::dialing(ep.clone(), &ep_label));
                let heard = Arc::new(crate::pool::Heard::new());
                let config = |compress| config_for(probe, compress, &trust);
                let checker = Checker {
                    trust: trust.clone(),
                    observed: Arc::clone(&observed),
                    stages: stages.clone(),
                    endpoint: ep.clone(),
                    label: ep_label.clone(),
                    heard: Arc::clone(&heard),
                };
                let r = match tcp_hop(&ep, &out).await {
                    Ok((tcp, far, via_proxy)) => {
                        stages.step(crate::step::TCP);
                        let config = config(apply && far && RUSSH_ZLIB_SOUND);
                        client::connect_stream(
                            config,
                            Hearing::new(tcp, Arc::clone(&heard)).telling(&stages),
                            checker,
                        )
                        .await
                        .map(|h| (h, far, via_proxy))
                        .map_err(|e| Said::new(stage_of_russh(&e), e.to_string()))
                    }
                    Err((stage, e)) => Err(Said::new(stage, e.to_string())),
                };
                if let Some(fp) = fingerprint_in(&observed) {
                    if let Ok(mut s) = seen.lock() {
                        *s = Some(fp);
                    }
                }
                match r {
                    Ok((session, far, via_proxy)) => Ok(Won {
                        session,
                        seen: observed,
                        winner: ep,
                        far,
                        via_proxy,
                        heard,
                    }),
                    Err(f) => {
                        stages.emit(crate::stage::failed(
                            ep.clone(),
                            &ep_label,
                            &format!("[{}] {}", f.code_or_fallback(), f.said),
                        ));
                        Err(f.wrap(|e| format!("{ep_label} {e}")))
                    }
                }
            });
        }
        let mut errors: Vec<String> = Vec::new();
        // 各地址没成的阶段：全输了按最说明问题的那一个记原因码（指纹不对 ＞ 解析不出 ＞ 拨不通 ＞ 超时 ＞ 别的）。
        let mut worst: Option<String> = None;
        let rank = |s: &str| {
            ["hostkey", "resolve", "tcp", "timeout"]
                .iter()
                .position(|x| *x == s)
                .unwrap_or(9)
        };
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(winner)) => {
                    set.abort_all();
                    stages.emit(crate::stage::won(
                        winner.winner.clone(),
                        &label(&winner.winner),
                    ));
                    return Ok(winner);
                }
                Ok(Err(f)) => {
                    let stage = f.code_or_fallback();
                    if worst.as_deref().is_none_or(|w| rank(stage) < rank(w)) {
                        worst = Some(stage.to_string());
                    }
                    errors.push(f.said)
                }
                Err(je) if je.is_cancelled() => {}
                Err(je) => errors.push(format!(
                    "{}: {je}",
                    copy_text("beConnect.race.taskFailed", &[])
                )),
            }
        }
        stages.note_why(crate::why_of_stage(worst.as_deref().unwrap_or("resolve")));
        // 句子只带原因词（按最说明问题的那一格）；各地址的原话进详情。
        Err(if errors.is_empty() {
            Said::from(copy_text("beConnect.race.noAddress", &[]))
        } else {
            Said::with_raw(
                copy_text(
                    "beConnect.race.allFailed",
                    &[
                        ("why", &race_reason(worst.as_deref())),
                        ("addrs", &addr_list.to_string()),
                    ],
                ),
                errors.join("\n"),
            )
        })
    };
    // ⚠ **这里没有握手看门狗**：本 crate 按调用形态禁 `timeout(`（`no_timer_guard`）。
    //   黑洞地址那一格由界面侧兜：它等 ack 有期限，到点就收掉本进程（drop 关 socket）。
    match race.await {
        Ok(w) => Ok(w),
        Err(e) => {
            let fp = seen.lock().ok().and_then(|g| g.clone());
            Err((e, fp))
        }
    }
}

/// 竞速全输了那一句的原因词（按最说明问题的那一格）：解析不出 · 拨不通 · 超时 · 主机指纹不符有词；别的说不清 ⇒ 原因不明。
pub fn race_reason(worst: Option<&str>) -> String {
    match worst {
        Some("resolve") => copy_text("reason.dial.resolve", &[]),
        Some("tcp") => copy_text("reason.dial.refused", &[]),
        Some("timeout") => copy_text("reason.link.noAnswer", &[]),
        Some("hostkey") => copy_text("reason.dial.hostKey", &[]),
        _ => copy_core::io_reason(std::io::ErrorKind::Other),
    }
}

/// 鉴权：配了钥匙走宿主给的那一样（钥匙 · 外部签名器 · 密码，[`env::KeySource`]）；没配走 ssh-agent（[`env::Agent`]）。
async fn authenticate(
    session: &mut client::Handle<Checker>,
    user: &str,
    key_path: Option<&str>,
    agent_sock: Option<&str>,
) -> Result<(), (Why, Said)> {
    let Some(key) = key_path.filter(|s| !s.trim().is_empty()) else {
        let best_hash = best_rsa_hash(session).await?;
        // agent 这一支**在当前线程上就地跑完**（`block_in_place` ＋ `block_on`），不留在外层 future 里。
        //
        // ⚠ 不是口味：拨号跑在要 `tokio::spawn` 的链路任务里（后端 `dial/link.rs` · 手机那份 FFI 外壳），外层 future 必须 `Send`；
        //   而 russh 的 `authenticate_publickey_with(.., &mut agent)` 经 `Signer::auth_sign(&AgentIdentity, ..)`
        //   那条高阶生命周期，rustc 证不出它的 future 是 `Send`（`implementation of Send is not general enough`，
        //   实打）。`block_on` 不要求 `Send` ⇒ 这一段就地跑完、不跨外层的任何 await。
        //   代价如实写：这一段期间占住一个 worker（鉴权是毫秒级到秒级的一趟），并且**取消不掉**
        //   （链路被关时它照样跑完这一趟鉴权；服务端的 `LoginGraceTime` 兜底）。
        //   宿主的运行时要是多线程的（后端 `main.rs` 的 `#[tokio::main]` · 手机那份两个 worker）；`block_in_place` 在单线程运行时上会 panic
        //   ⇒ 走到这一支的判据要用多线程运行时。
        let handle = tokio::runtime::Handle::current();
        return tokio::task::block_in_place(|| {
            handle.block_on(agent_auth(session, user, best_hash, agent_sock))
        });
    };
    let credential = env::get()
        .keys
        .credential(key)
        .map_err(|e| (Why::KeyUnreadable, e))?;
    let by_password = matches!(credential, Credential::Password(_));
    let authenticated = match credential {
        Credential::Key(key_pair) => {
            let best_hash = best_rsa_hash(session).await?;
            session
                .authenticate_publickey(
                    user,
                    PrivateKeyWithHashAlg::new(Arc::new(*key_pair), best_hash),
                )
                .await
                .map_err(|e| {
                    (
                        Why::Auth,
                        Said::with_raw(copy_text("beConnect.auth.keyFailed", &[]), &e),
                    )
                })?
        }
        Credential::Signer(signer) => {
            // 同 agent 那一支：`Signer` 那条高阶生命周期证不出 `Send` ⇒ 就地跑完。
            let handle = tokio::runtime::Handle::current();
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    let public = signer.public_key();
                    session
                        .authenticate_publickey_with(user, public, None, &mut External(signer))
                        .await
                })
            })
            .map_err(|e| {
                (
                    Why::Auth,
                    Said::with_raw(copy_text("beConnect.auth.keyFailed", &[]), &e.0),
                )
            })?
        }
        Credential::Password(password) => session
            .authenticate_password(user, password)
            .await
            .map_err(|e| {
                (
                    Why::Auth,
                    Said::with_raw(copy_text("beConnect.auth.passwordFailed", &[]), &e),
                )
            })?,
    };
    if !authenticated.success() {
        let said = if by_password {
            copy_text("beConnect.auth.passwordRejected", &[("user", user)])
        } else {
            copy_text("beConnect.auth.keyRejected", &[("user", user)])
        };
        return Err((rejected_why(&authenticated), said.into()));
    }
    Ok(())
}

/// RSA key 要协商出服务端支持的 hash；非 RSA 时 flatten 成 None。
async fn best_rsa_hash(session: &client::Handle<Checker>) -> Result<Option<HashAlg>, (Why, Said)> {
    Ok(session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| {
            (
                Why::Other,
                Said::with_raw(copy_text("beConnect.auth.rsaHash", &[]), &e),
            )
        })?
        .flatten())
}

/// 外部签名器（[`env::ExternalSigner`]）套成 russh 的 `Signer`：宿主回 DER，这里换成 SSH 的签名格式接在待签字节后面
/// （与 agent 那一形 `AgentClient::sign_request` 回的同形：`待签字节 ‖ string(string(算法) ‖ string(mpint r ‖ mpint s))`）。
struct External(Arc<dyn env::ExternalSigner>);

/// 外部签名没成（原话）。
#[derive(Debug)]
pub struct SignFailed(pub String);

impl From<russh::SendError> for SignFailed {
    fn from(e: russh::SendError) -> Self {
        SignFailed(format!("{e:?}"))
    }
}

/// 外部签名器只认这一种（手机 Keystore 里的 P-256）。
pub const EXTERNAL_ALGO: &str = "ecdsa-sha2-nistp256";

impl russh::Signer for External {
    type Error = SignFailed;

    fn auth_sign(
        &mut self,
        _key: &russh::keys::agent::AgentIdentity,
        _hash_alg: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let signer = Arc::clone(&self.0);
        async move {
            let der = signer
                .sign(to_sign.clone())
                .await
                .map_err(|e| SignFailed(e.logged()))?;
            let blob = ecdsa_der_to_ssh(&der).ok_or_else(|| SignFailed("DER".to_string()))?;
            let mut inner = Vec::new();
            put_string(&mut inner, EXTERNAL_ALGO.as_bytes());
            put_string(&mut inner, &blob);
            put_string(&mut to_sign, &inner);
            Ok(to_sign)
        }
    }
}

/// SSH 线上的 `string`：四字节大端长度 ＋ 本体。
fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// DER 的 `SEQUENCE { INTEGER r, INTEGER s }` ⇒ SSH 的 ECDSA 签名本体 `mpint r ‖ mpint s`（RFC 5656 §3.1.2）。
/// DER 的 INTEGER 本来就是最短的大端补码，与 mpint 的本体同形 ⇒ 只换外面那层长度。认不出 ⇒ `None`。
pub fn ecdsa_der_to_ssh(der: &[u8]) -> Option<Vec<u8>> {
    fn len_at(b: &[u8], i: usize) -> Option<(usize, usize)> {
        let first = *b.get(i)?;
        if first < 0x80 {
            return Some((usize::from(first), i + 1));
        }
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 2 {
            return None;
        }
        let mut len = 0usize;
        for k in 0..n {
            len = (len << 8) | usize::from(*b.get(i + 1 + k)?);
        }
        Some((len, i + 1 + n))
    }
    if *der.first()? != 0x30 {
        return None;
    }
    let (seq_len, mut i) = len_at(der, 1)?;
    if i + seq_len != der.len() {
        return None;
    }
    let mut out = Vec::new();
    for _ in 0..2 {
        if *der.get(i)? != 0x02 {
            return None;
        }
        let (n, at) = len_at(der, i + 1)?;
        let int = der.get(at..at + n)?;
        if int.is_empty() {
            return None;
        }
        put_string(&mut out, int);
        i = at + n;
    }
    (i == der.len()).then_some(out)
}

/// 公钥没过的原因码：服务端剩下的方法里没有公钥、只有密码 / 键盘交互 ⇒ 那台只收密码；否则密钥被拒。
pub fn rejected_why(res: &russh::client::AuthResult) -> Why {
    use russh::MethodKind as M;
    match res {
        russh::client::AuthResult::Failure {
            remaining_methods, ..
        } if !remaining_methods.contains(&M::PublicKey)
            && remaining_methods
                .iter()
                .any(|m| matches!(m, M::Password | M::KeyboardInteractive)) =>
        {
            Why::Password
        }
        _ => Why::Auth,
    }
}

/// ssh-agent 那一支的本体（见调用点：它**不在** `Send` 的 future 里跑）。
async fn agent_auth(
    session: &mut client::Handle<Checker>,
    user: &str,
    best_hash: Option<HashAlg>,
    agent_sock: Option<&str>,
) -> Result<(), (Why, Said)> {
    let Some(source) = env::get().agent.as_ref() else {
        return Err((
            Why::Auth,
            copy_text("beSshAgent.connect.unsupported", &[]).into(),
        ));
    };
    let mut agent = source.connect(agent_sock).await.map_err(|e| {
        (
            Why::Auth,
            // 原话格收 agent 那一层的那一句 ＋ 它的原话。
            Said::with_raw(copy_text("beConnect.agent.unreachable", &[]), e.logged()),
        )
    })?;
    let identities = agent.request_identities().await.map_err(|e| {
        (
            Why::Auth,
            Said::with_raw(copy_text("beConnect.agent.listFailed", &[]), &e),
        )
    })?;
    if identities.is_empty() {
        return Err((Why::Auth, copy_text("beConnect.agent.empty", &[]).into()));
    }
    let mut last_err: Option<(Why, Said)> = None;
    for id in identities {
        let pubkey = id.public_key().into_owned();
        match session
            .authenticate_publickey_with(user, pubkey, best_hash, &mut agent)
            .await
        {
            Ok(res) if res.success() => return Ok(()),
            Ok(res) => {
                last_err = Some((
                    rejected_why(&res),
                    copy_text("beConnect.agent.rejected", &[("user", user)]).into(),
                ))
            }
            Err(e) => {
                last_err = Some((
                    Why::Auth,
                    Said::with_raw(copy_text("beConnect.agent.signFailed", &[]), &e),
                ))
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        (
            Why::Auth,
            copy_text("beConnect.agent.allFailed", &[]).into(),
        )
    }))
}

/// 跳板那一步没成：外层那一句（哪一步）＋ 里层那一句与它的原话一起进详情（里层的句子不上外层的句子）。
fn inner(said: String, e: Said) -> Said {
    let raw = match e.raw {
        Some(raw) => format!("{}: {raw}", e.said),
        None => e.said,
    };
    Said::with_raw(said, raw)
}

/// 一台跳板在阶段行与报错里的名字（界面那边的配置标签；没有就 `host:port`）。
pub(crate) fn hop_name(hop: &JumpHop) -> String {
    if hop.label.is_empty() {
        label(&Endpoint {
            host: hop.host.clone(),
            port: hop.port,
        })
    } else {
        hop.label.clone()
    }
}

/// 跳板链，**从最外一台起**（本机直连的那一台在先，紧挨着目标的那一台在末）。`req.jump` 是紧挨目标的那一台，
/// 它的 `jump` 是再外一台（同 `ssh -J 外,内`）。
pub fn hop_chain(req: &DialRequest) -> Vec<&JumpHop> {
    let mut chain = Vec::new();
    let mut at = req.jump.as_ref();
    while let Some(hop) = at {
        chain.push(hop);
        at = hop.jump.as_deref();
    }
    chain.reverse();
    chain
}

/// 跳板那一步没成：记原因码、外层那一句（哪一步、哪一台）＋ 里层那一句与原话进详情。
fn jump_failed(
    stages: &StageSink,
    said: String,
    e: Said,
    fp: Option<String>,
) -> (Said, Option<String>) {
    stages.note_why(Why::Jump);
    (inner(said, e), fp)
}

/// 「跳板 {hop} 没连上」那一句。
fn hop_dial_failed(hop: &str) -> String {
    copy_text("beConnect.jump.dialFailed", &[("hop", hop)])
}

/// 经跳板链开到 `host:port` 的那条隧道。最外一台竞速（只它一个地址，判准答案拿回来给目标那条用 —— 跳板只运隧道、
/// 里面是已加密的字节，压不动）；其余逐台经上一台的 direct-tcpip 握手 ＋ 鉴权。
/// 回 `(隧道, 跨本机那一跳远不远（判准的答案）, 各台跳板的句柄（外在先）, 各台跳板那一条（外在先）)`；句柄**必须与目标连接同生命周期**。
/// 最外一台的 TCP 经 `out` 开（经没经代理记在它那一条上）；里头那几台在上一台的隧道里开，不经。
#[expect(
    clippy::result_large_err,
    reason = "失败带着给人看的整句（`Said`）原样往上交，一次拨号只走一次；手机那一侧共用这一形"
)]
async fn through_hops(
    req: &DialRequest,
    stages: &StageSink,
    out: &egress::Egress,
) -> Result<
    (
        russh::ChannelStream<client::Msg>,
        bool,
        Vec<client::Handle<Checker>>,
        Vec<JumpWire>,
    ),
    (Said, Option<String>),
> {
    let chain = hop_chain(req);
    let mut held: Vec<client::Handle<Checker>> = Vec::new();
    let mut wires: Vec<JumpWire> = Vec::new();
    let mut far = false;
    for hop in chain {
        let name = hop_name(hop);
        let hop_label = copy_text("beConnect.jump.label", &[("hop", &name)]);
        let hop_addrs = [Endpoint {
            host: hop.host.clone(),
            port: hop.port,
        }];
        stages.emit(crate::stage::dialing(hop_addrs[0].clone(), &hop_label));
        let hop_trust = Trust::of(&hop_addrs).map_err(|e| unreadable(stages, e))?;
        let (mut session, seen, wire, via_proxy) = match held.last() {
            None => {
                let won = race(req.probe, false, hop_trust, hop_addrs.to_vec(), stages, out)
                    .await
                    .map_err(|(e, fp)| jump_failed(stages, hop_dial_failed(&name), e, fp))?;
                far = won.far;
                (won.session, won.seen, won.heard, won.via_proxy)
            }
            Some(prev) => {
                let tunnel = open_tunnel(prev, &hop.host, hop.port, stages)
                    .await
                    .map_err(|e| jump_failed(stages, hop_dial_failed(&name), e, None))?;
                let observed: Seen = Arc::default();
                let checker = Checker {
                    trust: hop_trust.clone(),
                    observed: Arc::clone(&observed),
                    stages: stages.clone(),
                    endpoint: hop_addrs[0].clone(),
                    label: hop_label.clone(),
                    // 跳板那几台：断了 ⇒ 隧道跟着断 ⇒ 目标那条自己报断开；这一份没人等。
                    heard: Arc::new(crate::pool::Heard::new()),
                };
                let wire = Arc::new(crate::pool::Heard::new());
                let tunnel = Hearing::new(tunnel, Arc::clone(&wire)).telling(stages);
                let s = client::connect_stream(
                    config_for(req.probe, false, &hop_trust),
                    tunnel,
                    checker,
                )
                .await
                .map_err(|e| {
                    jump_failed(
                        stages,
                        hop_dial_failed(&name),
                        Said::with_raw(copy_text("beConnect.jump.targetFailed", &[]), &e),
                        fingerprint_in(&observed),
                    )
                })?;
                (s, observed, wire, false)
            }
        };
        authenticate(
            &mut session,
            &hop.user,
            hop.key_path.as_deref(),
            req.agent_sock.as_deref(),
        )
        .await
        .map_err(|(_, e)| {
            jump_failed(
                stages,
                copy_text("beConnect.jump.authFailed", &[("hop", &name)]),
                e,
                None,
            )
        })?;
        remember(&hop_addrs, &seen);
        stages.step(crate::step::AUTH);
        held.push(session);
        wires.push(JumpWire {
            endpoint: label(&hop_addrs[0]),
            heard: wire,
            via_proxy,
        });
    }
    let last = held.last().expect("调用方只在有跳板时走这里");
    let tunnel = open_tunnel(last, &req.host, req.port, stages)
        .await
        .map_err(|e| {
            stages.note_why(Why::Jump);
            (e, None)
        })?;
    Ok((tunnel, far, held, wires))
}

/// 经一条已鉴权的连接开 direct-tcpip 到 `host:port`。
async fn open_tunnel(
    via: &client::Handle<Checker>,
    host: &str,
    port: u16,
    stages: &StageSink,
) -> Result<russh::ChannelStream<client::Msg>, Said> {
    let opened = via
        .channel_open_direct_tcpip(
            host.to_string(),
            u32::from(port),
            "127.0.0.1".to_string(),
            0,
        )
        .await
        .map(russh::Channel::into_stream)
        .map_err(|e| {
            Said::with_raw(
                copy_text(
                    "beConnect.jump.tunnelFailed",
                    &[("host", host), ("port", &port.to_string())],
                ),
                &e,
            )
        });
    if opened.is_ok() {
        stages.step(crate::step::TUNNEL);
    }
    opened
}

/// 库读不出来：这一趟按「钥匙」拒（不当首见）。
fn unreadable(stages: &StageSink, e: Said) -> (Said, Option<String>) {
    stages.note_why(Why::HostKey);
    (e, None)
}

/// 连 ＋ 鉴权。`jump` 在就先连跳板链（[`through_hops`]）、经最后一台开 direct-tcpip 到目标主地址、在隧道上跑目标的握手
/// （fail-closed：跳板任一步失败就报错，**绝不**回落直连目标）。
#[expect(
    clippy::result_large_err,
    reason = "失败带着给人看的整句（`Said`）原样往上交，一次拨号只走一次；手机那一侧共用这一形"
)]
pub async fn establish(
    req: &DialRequest,
    stages: &StageSink,
) -> Result<Linked, (Said, Option<String>)> {
    establish_via(req, stages, egress::here()).await
}

/// [`establish`] 的本体：TCP 经 `out` 那一份出网描述开（生产是这台的 `egress::here()`；判据递假代理）。
#[expect(
    clippy::result_large_err,
    reason = "失败带着给人看的整句（`Said`）原样往上交，一次拨号只走一次；手机那一侧共用这一形"
)]
pub async fn establish_via(
    req: &DialRequest,
    stages: &StageSink,
    out: &egress::Egress,
) -> Result<Linked, (Said, Option<String>)> {
    // 目标那一台的全部地址：库里按它们认这台、记也写它们（经跳板时也一样，同一台机器）。
    let addrs = req.race_order();
    let target_trust = Trust::of(&addrs).map_err(|e| unreadable(stages, e))?;
    let (mut session, observed, winner, jumps, heard, far, via_proxy, jump_wires) =
        if req.jump.is_none() {
            // `apply = true` ⇒ 竞速交回的那一格就是落在这条上的答案。
            let w = race(req.probe, true, target_trust, addrs.clone(), stages, out).await?;
            (
                w.session,
                w.seen,
                w.winner,
                Vec::new(),
                w.heard,
                w.far,
                w.via_proxy,
                Vec::new(),
            )
        } else {
            let (tunnel, far_inner, jumps, jump_wires) = through_hops(req, stages, out).await?;
            let observed: Seen = Arc::default();
            let ep_label = copy_text(
                "beConnect.jump.target",
                &[("host", &req.host), ("port", &req.port.to_string())],
            );
            // 目标那条的字节经隧道到：收包时刻记在隧道这一层（跳板那条 TCP 上的字节不全是它的）。
            let heard = Arc::new(crate::pool::Heard::new());
            let winner = Endpoint {
                host: req.host.clone(),
                port: req.port,
            };
            let checker = Checker {
                trust: target_trust.clone(),
                observed: Arc::clone(&observed),
                stages: stages.clone(),
                endpoint: winner.clone(),
                label: ep_label,
                heard: Arc::clone(&heard),
            };
            let session = client::connect_stream(
                config_for(req.probe, far_inner && RUSSH_ZLIB_SOUND, &target_trust),
                Hearing::new(tunnel, Arc::clone(&heard)).telling(stages),
                checker,
            )
            .await
            .map_err(|e| {
                stages.note_why(crate::why_of_stage(stage_of_russh(&e)));
                (
                    Said::with_raw(copy_text("beConnect.jump.targetFailed", &[]), &e),
                    fingerprint_in(&observed),
                )
            })?;
            (
                session, observed, winner, jumps, heard, far_inner,
                // 目标那条在跳板里头开（direct-tcpip）：它自己的 TCP 不经代理，经没经记在最外那台跳板那一条上。
                false, jump_wires,
            )
        };
    let fingerprint = fingerprint_in(&observed);
    if let Err(e) = authenticate(
        &mut session,
        &req.user,
        req.key_path.as_deref(),
        req.agent_sock.as_deref(),
    )
    .await
    {
        let (code, e) = e;
        stages.note_why(code);
        stages.emit(crate::stage::auth_failed(&e.said));
        return Err((e, fingerprint));
    }
    remember(&addrs, &observed);
    stages.step(crate::step::AUTH);
    stages.emit(crate::stage::auth_ok());
    stages.emit(crate::stage::established());
    Ok(Linked {
        session,
        fingerprint,
        endpoint: label(&winner),
        winner,
        _jumps: jumps,
        budget: crate::pool::Budget::new(),
        held: Mutex::new(Vec::new()),
        idle_sftp: Mutex::new(None),
        heard,
        far,
        via_proxy,
        jump_wires,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/dial/link_meter_tests.rs"]
mod link_meter_tests;

#[cfg(test)]
#[path = "../../../tests/backend/dial_compress_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/dial_egress_tests.rs"]
mod egress_tests;
