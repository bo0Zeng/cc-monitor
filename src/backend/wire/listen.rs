//! 常驻监听 —— 脱离宿主之后，backend 还能被找到、被问到、被接上（`src/doc/INVARIANTS.md` § 48.1：门由内核给）。
//! stdio 载体上宿主一退读端就断、backend 随即 broken-pipe 退出；难的不是怎么脱离，是脱离之后还怎么跟它对话。
//!
//! # 走 Unix 套接字，门由内核给
//!
//! 套接字住这台家里只给本人的目录（`relay_route_core::listen_socket_for`，目录 `0700`），收下的每条连接再核一次对端 uid
//! （共享 crate `own-chan`）⇒ 连得上的只有本账号自己的进程，门由内核给，不另设钥匙。
//! 远端经 ssh 跑一次 `ccm -- --resident-attach`（`control/resident.rs` 那个小中继）连它：ssh 已经证明了「是本人」。
//! 起它的：本机 `src/frontend/shell/src/local_backend_host.rs`，远端 `--resident-ensure`（`control/resident.rs`）—— 同一种交法：只交 [`ENV_RESIDENT`]。
//!
//! # 两档连接，hello 写在分档之前
//!
//! - 流：attach 行形状对 ⇒ 这条连接接管出 / 入两个方向（[`admit`]），回话里带这台相对客户端那一版的新旧；
//!   这台一份 watcher，每条连接各一份投影（出方向通道 · 名单 · 丢帧账）/ inbound / writer（`stream/entry.rs::serve_listening`），几个前端可以同时挂着。
//! - 不限次的「只读 hello 就走」：连上就有 hello，读完即关。
//!
//! hello 写在分档之前：「有没有长驻后端」靠读一行 hello 答，协议一个字节都不用加。
//! 多客户是「整台一份 watcher、扇出到每条连接各自那一条通道」（`single_stream_guard.rs` 钉着「源码里恰好一份、按连接实例化」）。
//!
//! # 边界
//!
//! 1. 一台机器一个家一个常驻后端：目录独占锁抢不到就带 [`EXIT_ADDR_IN_USE`] 退出，绝不换个地方再起一个
//!    （换地方 = 每台机 N 个 backend，中转口与全部 SSH 各 N 份）。宿主连上去先读 hello 比对，对不上就出声并拒绝。
//! 2. 一个定时器都没有：`accept` 与读一行都阻塞在内核事件上（`src/backend/clippy.toml` 的 `disallowed-methods`）。
//! 3. 别的 uid 连进来（root 例外不了：内核报的就是它的 uid）⇒ 关掉、出声，hello 都不给。

use cmd_wire::attach::{AttachLine, AttachReply};
use relay_route_core::{Cap, Over};

/// 宿主告诉后端「你是常驻的那一个」的 env 名（值 `1`）。听哪个套接字不交：按家算，宿主与后端同一个函数。
pub const ENV_RESIDENT: &str = crate::platform::child_env::RESIDENT;

/// 远端 `--resident-ensure` 起常驻后端时交的中转口 env 名（`main.rs` 在库外，够不着 `relay::` 的 crate 内口）。
pub const RELAY_PORT_ENV: &str = crate::relay::ENV_PORT;

/// 目录独占锁抢不到（已有一个常驻后端在听）的退出码。**与「起不来」区分开**：
/// 宿主看到它就知道「已经有人在听了」，该去连而不是再起一个。
pub const EXIT_ADDR_IN_USE: i32 = 3;

/// 常驻配置不成立（开关值不认得 · 沙箱跑却要占真家 · 绑不上）的退出码。**fail closed**：宁可不起。
pub const EXIT_BAD_LISTEN_CONFIG: i32 = 4;

/// 一行 attach 请求的字节上限。请求形如 `{"attach":true,"tz":…,"mine":…}`（几十字节），8 KiB 给了两个量级余量。
/// 少了它就是一个无界堆分配：对端可以一直发字节不发换行。
/// 超限：拒收 + 回错（关连接并出声，不静默截断成一行看起来对的 JSON）。读这一行经 `comms_inward::line::read_capped_line`。
pub const ATTACH_LINE: Cap = Cap {
    bytes: 8 * 1024,
    over: Over::Reject,
};

/// 拒绝的理由（闭集）：`refusal_line` 只拼这里的常量，没有任何一段外来字节
/// 会进到那行 JSON 里（由 `refusal_reasons_are_a_closed_set` 钉住）。
/// 常驻后端自己只发这一个：attach 行形状不对。
pub const REFUSE_MALFORMED: &str = "malformed-attach";
/// 小中继（`--resident-attach`）断开：没人在听（刚起的还没绑上 / 起来就退了）。monitor 隔一会儿再接。
pub const REFUSE_ABSENT: &str = "absent";
/// 小中继断开：别的原因（不归本人 · 路径坏了）。再接也一样。
pub const REFUSE_UNREACHABLE: &str = "unreachable";

/// `--resident-call` 那一行 stdin：问哪条帧命令 ＋ 它的入参。发的一方（`remote_ask::ask_resident`）与收的一方（`control/resident.rs::call`）同一个类型。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResidentCall {
    pub(crate) cmd: String,
    #[serde(default)]
    pub(crate) args: serde_json::Value,
}

/// `--resident-call` 的码：这台没有常驻在听 ⇒ 照实失败（不在这台起一个冷进程顶上）。
pub(crate) const CALL_ABSENT: &str = "resident_absent";
/// `--resident-call` 的码：等到期限，常驻还没回。
pub(crate) const CALL_TIMED_OUT: &str = "timed_out";

/// 那一台（`machine` ＝ 给人看的称呼）的常驻没在跑。
pub(crate) fn call_absent(machine: &str) -> String {
    copy_core::copy_text("beResident.call.absent", &[("machine", machine)])
}

/// 那一台的常驻在期限 `after` 内没回。
pub(crate) fn call_timed_out(machine: &str, after: crate::platform::child::Deadline) -> String {
    copy_core::copy_text(
        "beResident.call.timedOut",
        &[
            ("machine", machine),
            (
                "dur",
                &copy_core::format_duration(after.shown_secs().saturating_mul(1000)),
            ),
        ],
    )
}

/// backend 这次跑成什么形态。由环境决定，不由 argv 决定：换的是同一个流模式的载体，不是新增一条子命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// stdin/stdout 一对管道，宿主一退就死（被监护的本机后端 · 远端 `--stream`）。
    Stdio,
    /// 常驻：听这台家里那个套接字。
    Listen,
}

/// 从环境算出形态。纯函数（`get` 是入参）。
/// - 没有 / 空串 ⇒ [`Mode::Stdio`]（shell 里 `export X=` 是常态）。
/// - `1` ⇒ [`Mode::Listen`]。
/// - 别的值 ⇒ `Err`：多半是接线写错了，静默退回 stdio 会让「我明明开了常驻」变成一个查不出来的谜。
pub fn mode_from(get: &dyn Fn(&str) -> Option<String>) -> Result<Mode, String> {
    match get(ENV_RESIDENT).as_deref().map(str::trim) {
        None | Some("") => Ok(Mode::Stdio),
        Some("1") => Ok(Mode::Listen),
        Some(other) => Err(crate::common::contract::malformed(&format!(
            "{ENV_RESIDENT}={other:?} is neither empty nor 1; refusing to guess the carrier"
        ))),
    }
}

/// 拒绝那一行。`reason` 只许是本模块的常量。
pub fn refusal_line(reason: &'static str) -> String {
    AttachReply::Refused {
        reason: reason.to_string(),
    }
    .line()
}

/// 这条连接给什么档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admit {
    /// 交出流：看的那一台的时区（原样，认不得 ⇒ UTC 由读的那一处定）· 回话里那一格（这台相对 attach 行的 `mine`；没带 ⇒ 缺）·
    /// 在看名单（这条流从第一帧起照它发；没带 ⇒ 全发）。
    Stream {
        tz: Option<String>,
        reply: AttachReply,
        watch: Option<crate::wire::WatchAsk>,
    },
    /// 拒绝，并把理由写回去（**出声**，不是静默 FIN —— 照 `relay::serve` 那条 503 的形状）。
    Refuse(&'static str),
}

/// 判一行 attach 请求、分档。**纯函数**。形状是 `cmd_wire::attach::AttachLine`（认不得的格 · `attach` 不是 `true` · 不是 JSON ⇒ malformed）；
/// 多客户：形状对就交流，不看「有没有人占着」。新旧按 `deploy_contract::build_relation(own, mine)` 判（序键是 `BUILD_ID` 的，哪一版后端算都一样）。
/// 要了 deflate ⇒ 这台总认（回话带那一格，之后那条流经 [`Down`] 压）。
pub fn admit(line: &str, own: &str) -> Admit {
    match serde_json::from_str::<AttachLine>(line.trim()) {
        Ok(l) => Admit::Stream {
            reply: AttachReply::Ok {
                relation: l
                    .mine
                    .as_deref()
                    .map(|m| deploy_contract::build_relation(own, m)),
                deflate: l.deflate,
            },
            tz: l.tz,
            watch: l.watch,
        },
        Err(_) => Admit::Refuse(REFUSE_MALFORMED),
    }
}

/// 此刻连着的流（多客户）。连接号从 1 起（0 留给空转那一份 watcher 的槽位）；
/// 「最后一个客户走了」= [`Clients::leave`] 回 0 —— 不是「起我的那个 monitor 退了」。
#[derive(Debug, Default)]
pub struct Clients {
    live: std::collections::BTreeSet<u64>,
    next: u64,
}

impl Clients {
    /// 接上一条：回它的连接号。
    pub fn join(&mut self) -> u64 {
        self.next += 1;
        self.live.insert(self.next);
        self.next
    }

    /// 走了一条：回还剩几条（同一个号走两次不重复扣）。
    pub fn leave(&mut self, id: u64) -> usize {
        self.live.remove(&id);
        self.live.len()
    }

    /// 此刻几条。
    pub fn count(&self) -> usize {
        self.live.len()
    }
}

/// 往一条连接写一行并 flush。本模块只有这一处 `write_all(`（在 `wire.rs`「出方向帧只许有一个写者」那条判据的人群里）；
/// 它写的不是出方向帧：握手行发生在 `writer_task` 起来之前，或者根本不给流。
pub async fn write_line<W>(w: &mut W, line: &str) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

/// deflate 级别（同 OpenSSH ≤ 10.5 的 `zlib@openssh.com`）。只管一帧之内压多狠：每帧一个上下文（[`Down`]），帧之间不回指。
pub const DEFLATE_LEVEL: u32 = 6;

/// 压缩器一次往外攒多少字节（攒满就先往下写，再接着压）。
const DEFLATE_CHUNK: usize = 64 * 1024;

/// 接成流之后这台写过去的那一半：原样，或客户端要了 deflate（回话认了）⇒ 回话那一行之后整条是一个 raw deflate 流。
/// 每帧一个上下文：一帧是一行，`poll_write` 一次至多喂到这一行的换行为止；换行一喂进去，下一个字节之前先 full flush，
/// 清掉压缩器的字典 ⇒ 下一帧一个字节都回指不到前面的帧，同一次排空里写下的几帧也一样。外来内容（抓回来的网页 · 别人写的文件）
/// 就借密文长度猜不到别的帧里的秘密。解的那一侧照样是一个 raw inflate 流，不用跟着换。
/// `poll_flush` 收尾那一帧（写到半行也切一刀）再 flush 下面 ⇒ `writer_task` 三路排空才 flush（一直排着时每 64 KB 一次），
/// flush 过的帧到了对端就解得出来；几帧一批仍是一次写系统调用。
pub struct Down<W> {
    inner: W,
    z: Option<Deflating>,
}

struct Deflating {
    z: flate2::Compress,
    /// 压出来、还没写下去的字节（`out[at..]`）。
    out: Vec<u8>,
    at: usize,
    /// 上一刀之后喂过字节、这一帧的换行还没到。
    dirty: bool,
    /// 该切一刀（一帧喂完了，或要 flush 了），full flush 还没做完。
    cut: bool,
}

impl<W> Down<W> {
    pub fn new(inner: W, deflate: bool) -> Self {
        Down {
            inner,
            z: deflate.then(|| Deflating {
                z: flate2::Compress::new(flate2::Compression::new(DEFLATE_LEVEL), false),
                out: Vec::with_capacity(DEFLATE_CHUNK),
                at: 0,
                dirty: false,
                cut: false,
            }),
        }
    }
}

/// 攒着的那几字节写下去（写完清空，好接着压）。
fn drain<W: tokio::io::AsyncWrite + Unpin>(
    inner: &mut W,
    d: &mut Deflating,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<std::io::Result<()>> {
    while d.at < d.out.len() {
        let n = std::task::ready!(std::pin::Pin::new(&mut *inner).poll_write(cx, &d.out[d.at..]))?;
        if n == 0 {
            return std::task::Poll::Ready(Err(std::io::ErrorKind::WriteZero.into()));
        }
        d.at += n;
    }
    d.out.clear();
    d.at = 0;
    std::task::Poll::Ready(Ok(()))
}

/// 攒着的写下去，该切的那一刀切完（full flush 吐的攒满了 ⇒ 写下去再接着收）。
fn settle<W: tokio::io::AsyncWrite + Unpin>(
    inner: &mut W,
    d: &mut Deflating,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<std::io::Result<()>> {
    loop {
        std::task::ready!(drain(inner, d, cx))?;
        if !d.cut {
            return std::task::Poll::Ready(Ok(()));
        }
        d.z.compress_vec(&[], &mut d.out, flate2::FlushCompress::Full)
            .map_err(deflate_failed)?;
        d.cut = d.out.len() == d.out.capacity();
    }
}

fn deflate_failed(e: flate2::CompressError) -> std::io::Error {
    std::io::Error::other(e)
}

impl<W: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Down<W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let Some(d) = this.z.as_mut() else {
            return std::pin::Pin::new(&mut this.inner).poll_write(cx, buf);
        };
        // 这一次至多喂到这一帧的换行（含）：后面的字节属于下一帧，要等切过一刀再喂。
        let frame = buf
            .iter()
            .position(|&b| b == b'\n')
            .map_or(buf, |i| &buf[..=i]);
        loop {
            std::task::ready!(settle(&mut this.inner, d, cx))?;
            let was = d.z.total_in();
            d.z.compress_vec(frame, &mut d.out, flate2::FlushCompress::None)
                .map_err(deflate_failed)?;
            let took = (d.z.total_in() - was) as usize;
            if took > 0 {
                if took == frame.len() && frame.last() == Some(&b'\n') {
                    d.cut = true;
                    d.dirty = false;
                } else {
                    d.dirty = true;
                }
                return std::task::Poll::Ready(Ok(took));
            }
            if buf.is_empty() {
                return std::task::Poll::Ready(Ok(0));
            }
            // 一个字节没吃：压缩器先吐的那一截占满了 ⇒ 写下去再喂；什么都没吐 ⇒ 压缩器卡住了。
            if d.out.is_empty() {
                return std::task::Poll::Ready(Err(std::io::Error::other(
                    "deflate made no progress",
                )));
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if let Some(d) = this.z.as_mut() {
            // 写到半行就 flush ⇒ 也切一刀（压过的字节要当场解得出来）。
            if d.dirty {
                d.cut = true;
                d.dirty = false;
            }
            std::task::ready!(settle(&mut this.inner, d, cx))?;
        }
        std::pin::Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::ready!(self.as_mut().poll_flush(cx))?;
        std::pin::Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/wire/listen_tests.rs"]
mod tests;
