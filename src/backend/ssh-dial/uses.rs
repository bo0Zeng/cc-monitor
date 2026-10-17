//! 开出 channel 之后的几种用法（[`crate::Use`]）。握手已过：每一支先回 ack（失败就回失败的 ack），再干自己那件事。
//! I/O 是调用方给的一对 `AsyncRead` / `AsyncWrite`（常驻后端里是那条链路的两根内存管子，`link.rs`）；连接是池里拿的（[`Lease`]，同身份复用）。

use crate::Said;
use copy_core::copy_text;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::connect::{self, Linked};
use crate::{
    pool, write_line, write_stages_then_ack, CaptureOpts, Captured, DialAck, DialRequest,
    StageSink, Use, ACK_V, USES,
};

fn ok_ack(l: &Linked) -> DialAck {
    DialAck {
        ok: true,
        error: None,
        detail: None,
        fingerprint: l.fingerprint.clone(),
        endpoint: Some(l.endpoint.clone()),
        winner: Some(l.winner.clone()),
        reason: None,
        key_type: None,
        expected_fingerprint: None,
        v: ACK_V,
        uses: USES,
    }
}

/// `channel.exec(true, cmd)` 装进一个显式 `Send` 的盒子：russh 的 `Channel::exec` 是借 `&self` 的 `async fn`，放进要 `tokio::spawn`
/// 的链路任务里会撞 rustc 的「`implementation of Send is not general enough`」。在这里把 Send 写死在一个具体生命周期上。
fn exec(
    channel: &russh::Channel<russh::client::Msg>,
    cmd: Vec<u8>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(channel.exec(true, cmd))
}

/// 同上，但给**写半边**（capture 那一臂先 `split` 再 exec：读半边交给 [`collect`]，写半边交给 [`CloseOnDrop`]）。
fn exec_half(
    w: &russh::ChannelWriteHalf<russh::client::Msg>,
    cmd: Vec<u8>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(w.exec(true, cmd))
}

/// 能被「关掉」的那一半（生产 = russh 的 `ChannelWriteHalf`；判据用记账替身）。
pub trait Closable: Send + 'static {
    fn close_it(self) -> impl std::future::Future<Output = ()> + Send;
}

impl Closable for russh::ChannelWriteHalf<russh::client::Msg> {
    fn close_it(self) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let _ = self.close().await;
        }
    }
}

/// 被丢 ⇒ 向远端发一次关通道（与 russh 给 `into_stream` 那一形的 `ChannelCloseOnDrop` 同形）。
///
/// russh 0.61 的裸 `Channel` 被丢不发 `CHANNEL_CLOSE`。capture 那一臂拿的是裸通道 ⇒ 链路被关或 `abort_marker` 提前收工时，
/// 本地那一格已经还回预算，远端那条 session 通道与它上面的进程却还开着 ⇒ 下一次开通道被远端回拒、这条连接的上限被学小。
/// 关通道是尽力的：发出去了，对面怎么收场是它的事。
pub struct CloseOnDrop<W: Closable>(Option<W>);

impl<W: Closable> CloseOnDrop<W> {
    pub fn new(w: W) -> Self {
        CloseOnDrop(Some(w))
    }

    pub fn get(&self) -> &W {
        self.0.as_ref().expect("CloseOnDrop 只在 Drop 里取走")
    }
}

impl<W: Closable> Drop for CloseOnDrop<W> {
    fn drop(&mut self) {
        let Some(w) = self.0.take() else {
            return;
        };
        // Drop 里不能 await ⇒ 交一个任务去发（同 russh `ChannelCloseOnDrop`）。没有运行时（进程在收尾）就不发了。
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(w.close_it());
        }
    }
}

/// 一条 session 通道（或一条隧道）走哪一道 —— 住 `pool.rs`（放置要按它判），这里转一手。
pub use crate::pool::Lane;

/// 一份拨号请求的用法 ⇒ 它在池里走哪一道。
pub fn lane_of(u: Use) -> Lane {
    match u {
        Use::Stream => Lane::Stream,
        Use::Capture => Lane::Query,
        Use::Shell => Lane::Stream,
        // 不开通道（`MaxSessions` 不数它），同隧道那一道。
        Use::Hold => Lane::Tunnel,
        Use::Forward => Lane::Tunnel,
    }
}

/// 这一趟手里的那条 SSH 连接：池里放置来的（复用 / 新拨 / 多开），或者这一趟就地拨的（测试连接）。
pub struct Lease {
    /// 池里的身份；`None` = 不进池（测试连接那一形：它要看的就是一次真拨号的阶段）。
    key: Option<String>,
    linked: Arc<Linked>,
    /// 这一趟走哪一道（换连接重放时照同一道放）。
    lane: Lane,
    /// 放置时借到的那一格（隧道没有）。开出通道之后随通道交给调用方。
    permit: Option<pool::Permit>,
    /// 这条是复用来的（不是这一趟拨的）。只有复用来的，开 channel 失败（连接死了那一形）才值得重拨一次。
    reused: bool,
}

/// 开 session 通道失败的这一形是不是「**远端说满了**」（`MaxSessions`：OpenSSH 回 `administratively prohibited`，
/// sshd 日志原话 `no more sessions`；`resource shortage` 同一类）—— 连接本身好好的，只是这条连接上不能再多开。
fn refused_by_remote(e: &russh::Error) -> bool {
    matches!(
        e,
        russh::Error::ChannelOpenFailure(
            russh::ChannelOpenFailure::AdministrativelyProhibited
                | russh::ChannelOpenFailure::ResourceShortage
        )
    )
}

/// 多久没收到对端字节（毫秒）算「久」：过了它，复用这条连接开通道时**与新建一条同时起跑**（[`race`]）。
///
/// 值照保活定：健康的空闲连接每 `KEEPALIVE`（10 秒）没收包就发一次保活、一个往返后收到回话 ⇒ 它最后收包的年龄落在
/// 0 到「10 秒 ＋ 一个往返」之间（用户的远端往返 12–577 ms）。门槛再留 2 秒：保活发出去 2 秒还没回话，才当它可能是黑洞。
/// 门槛（12 秒）在判死（保活 `KEEPALIVE_MAX + 1` 轮，20 秒）之前：这一段里开通道，赛跑接住；判死之后开通道的那一下当场失败、换新连接。
/// 判错的代价不对称：健康的被判成久 ⇒ 多起一条 TCP（旧的先回话，新的当场丢掉）；黑洞没判出来 ⇒ 干等到保活判死（保活 `KEEPALIVE_MAX + 1` 轮，最后收包之后 20 秒）。
pub const STALE_AFTER_MS: u64 = connect::KEEPALIVE_MS + 2_000;

/// 这条连接（`silent_ms` 毫秒没收到对端字节）开通道之前要不要和新建赛跑。
pub fn stale(silent_ms: u64) -> bool {
    silent_ms >= STALE_AFTER_MS
}

/// 等回话那一段，旧连接回的是「开不成」、而连接也已经断开 ⇒ **它死了**（保活连着没回话被 russh 判死 —— 挂在它上面的开通道
/// 随之失败；或对端断了）。这一形照「判死」记：族里那之后也没动静的成员一起摘（`pool::Watch::dead`），重开落到新连接上。
/// 远端说满了（连接还活着）· 开成了 ⇒ 不算。
pub fn died_waiting<T>(r: &Result<T, russh::Error>, closed: bool) -> bool {
    r.is_err() && closed
}

/// [`race`] 的结局。
pub enum Raced<O, N> {
    /// 旧连接先回话（成或不成，都是它的回话）。
    Old(O),
    /// 新的一条先站住。
    New(N),
}

/// **赛跑**：旧连接上等回话（`old`）与新建一条（`fresh`）同时起跑，谁先站住用谁。同时好了 ⇒ 旧的（不白换）；
/// 新建失败不算数 ⇒ 接着等旧的。另一边随返回被丢掉（新建到一半的那条当场关掉）。不靠钟：两边都是事件。
pub async fn race<O, N, E: std::fmt::Debug>(
    old: impl std::future::Future<Output = O>,
    fresh: impl std::future::Future<Output = Result<N, E>>,
) -> Raced<O, N> {
    tokio::pin!(old, fresh);
    let mut fresh_failed = false;
    loop {
        tokio::select! {
            biased;
            o = &mut old => return Raced::Old(o),
            n = &mut fresh, if !fresh_failed => match n {
                Ok(n) => return Raced::New(n),
                Err(e) => {
                    tracing::info!("dial: 赛跑里新建的那条没成（{e:?}）—— 接着等旧连接回话");
                    fresh_failed = true;
                }
            },
        }
    }
}

/// 开通道那一下的结局：旧连接的回话（成或不成），或者赛跑里新的一条先站住了（连同这一族的身份）。
enum Answered {
    Old(Result<russh::Channel<russh::client::Msg>, russh::Error>),
    Replaced(Box<Linked>, String),
}

impl Lease {
    /// 拿一条连接。`probe` / `stages` ⇒ 不进池、就地拨；其余按身份在池里放置（`pool::Pool::place`：复用 / 按需多开）。
    pub async fn take(
        req: &DialRequest,
        stages: &StageSink,
        lane: Lane,
    ) -> Result<Lease, (Said, Option<String>)> {
        // 握着连接那一形要进池（别的链路复用它）：阶段行照报，只是复用到的那一趟没有拨号的阶段可报。
        if req.probe || (req.stages && req.use_ != Use::Hold) {
            let linked = Arc::new(connect::establish(req, stages).await?);
            let permit = linked.budget.try_take(lane);
            return Ok(Lease {
                key: None,
                linked,
                lane,
                permit,
                reused: false,
            });
        }
        Self::place(req, stages, pool::identity(req), lane).await
    }

    async fn place(
        req: &DialRequest,
        stages: &StageSink,
        key: String,
        lane: Lane,
    ) -> Result<Lease, (Said, Option<String>)> {
        Self::place_with(key, lane, || connect::establish(req, stages)).await
    }

    /// 同 [`Lease::place`]，拨号由调用方给（赛跑赢下来的那条：第一次要拨时交出它）。
    async fn place_with<F, Fut>(
        key: String,
        lane: Lane,
        dial: F,
    ) -> Result<Lease, (Said, Option<String>)>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<Linked, (Said, Option<String>)>>,
    {
        let placed = pool::ssh().place(&key, lane, dial).await?;
        let (s, t, cap) = placed.conn.budget.free();
        tracing::info!(
            "dial: {} {} 的 SSH 连接（{lane:?}；这条通道剩 {s}/{cap}、车道剩 {t}；池里此刻 {} 条）",
            match placed.how {
                pool::How::Reused => "复用",
                pool::How::Fresh => "新拨了",
                pool::How::Extra(pool::Why::Full) => "通道都满了，多开了一条到",
                pool::How::Extra(pool::Why::Bulk) => "传输分道，多开了一条批量连接到",
            },
            placed.conn.endpoint,
            pool::ssh().live()
        );
        Ok(Lease {
            key: Some(key),
            linked: placed.conn,
            lane,
            permit: placed.permit,
            reused: placed.how == pool::How::Reused,
        })
    }

    /// 这一趟手里那条连接（调用方要把它攥到通道用完为止 —— 句柄一 drop 整条连接就断）。
    pub fn linked(&self) -> &Arc<Linked> {
        &self.linked
    }

    /// 这一趟走哪一道（`sftp.rs` 据它判「用完停不停进空位」：只有传输停）。
    pub fn lane(&self) -> Lane {
        self.lane
    }

    /// 开一条 session channel。那一格**放置时已经借好了**（`pool::Budget`：长流 · 查询 · SFTP 同一条连接、同一道闸），
    /// 随返回的 [`pool::Permit`] 走，调用方攥到通道用完为止。
    ///
    /// 开失败分两形：
    /// - **远端说满了**（`MaxSessions`）⇒ 这条连接学到上限（`Budget::refused`：空格当场作废）、**不摘它**
    ///   （长流还在它上面），照同一道重新放置 —— 通常落到一条新连接上（`Why::Full`）；学到的上限是 0 ⇒ 报错（远端根本不给开）。
    /// - **连接死了**（其余）⇒ 复用来的才摘掉、重拨一次（同一机制换一条新连接）；新拨的那条再失败就如实报。
    ///
    /// 复用来的连接**久没收到对端字节**（[`stale`]）⇒ 开通道与新建一条同时起跑（[`race`]）：旧的先回话照常用；
    /// 新的先站住 ⇒ 旧的判死（连同族里那之后也没动静的成员一起摘，`pool::Watch::dead`），这一趟改放到新的那条上。
    /// 路断成黑洞时，不用再干等界面的握手期限。
    pub async fn session_channel(
        &mut self,
        req: &DialRequest,
        stages: &StageSink,
    ) -> Result<(russh::Channel<russh::client::Msg>, pool::Permit), Said> {
        for _ in 0..=pool::MAX_CONNECTIONS_PER_HOST {
            let Some(permit) = self.permit.take() else {
                return Err(copy_text("beUses.session.noSlot", &[]).into());
            };
            // 等远端回「开好了」这一段被打断（链路被关）⇒ 摘掉这条连接（`pool::Watch`）。
            // 只在池里复用来的连接上赛跑（这一趟拨的那条刚握完手；不进池的那一形没有可换的）。
            let racing = self
                .key
                .clone()
                .filter(|_| self.reused && stale(self.linked.heard.silent_for()));
            let opened = {
                let watch = self
                    .key
                    .as_deref()
                    .map(|k| pool::Watch::new(pool::ssh(), k, &self.linked));
                let open = self.linked.session.channel_open_session();
                let answered = match racing {
                    Some(key) => {
                        tracing::info!(
                            "dial: {} 上 {} ms 没收到对端字节 —— 开通道与新建一条同时起跑",
                            self.linked.endpoint,
                            self.linked.heard.silent_for()
                        );
                        // 赛跑里那条新建的阶段与原因码不进这一趟（它多半会被丢掉）；进展照报（这一趟在等它，它在动就是这一趟在动）。
                        let quiet = stages.quiet();
                        match race(open, connect::establish(req, &quiet)).await {
                            Raced::Old(r) => Answered::Old(r),
                            Raced::New(fresh) => Answered::Replaced(Box::new(fresh), key),
                        }
                    }
                    None => Answered::Old(open.await),
                };
                match (&answered, watch) {
                    (Answered::Old(r), Some(w))
                        if died_waiting(r, self.linked.session.is_closed()) =>
                    {
                        w.dead(pool::DeadBy::Broke);
                    }
                    (Answered::Old(_), Some(w)) => w.done(),
                    (Answered::Replaced(..), Some(w)) => w.dead(pool::DeadBy::Outrun),
                    (_, None) => {}
                }
                answered
            };
            let opened = match opened {
                Answered::Old(r) => r,
                Answered::Replaced(fresh, key) => {
                    drop(permit);
                    let mut won = Some(fresh);
                    *self = Self::place_with(key, self.lane, || {
                        let won = won.take();
                        async move {
                            match won {
                                Some(l) => Ok(*l),
                                None => connect::establish(req, stages).await,
                            }
                        }
                    })
                    .await
                    .map_err(|(e, _)| e)?;
                    continue;
                }
            };
            let e = match opened {
                Ok(c) => {
                    stages.step(crate::step::CHANNEL);
                    return Ok((c, permit));
                }
                Err(e) => e,
            };
            drop(permit);
            let Some(key) = self.key.clone() else {
                return Err(Said::with_raw(
                    copy_text("beUses.session.openFailed", &[]),
                    &e,
                ));
            };
            if refused_by_remote(&e) && !self.linked.session.is_closed() {
                let cap = self.linked.budget.refused();
                tracing::warn!(
                    "dial: 远端回拒了 {} 上的一条 session 通道（{e}）—— 这条连接学到上限 {cap}，换一格放",
                    self.linked.endpoint
                );
                if cap == 0 {
                    return Err(Said::with_raw(
                        copy_text("beUses.session.refusedAll", &[]),
                        &e,
                    ));
                }
            } else if self.reused {
                tracing::warn!("dial: 复用的连接上开 channel 失败（{e}）—— 摘掉它、重拨一次");
                pool::ssh().evict(&key, &self.linked);
            } else {
                return Err(Said::with_raw(
                    copy_text("beUses.session.openFailed", &[]),
                    &e,
                ));
            }
            *self = Self::place(req, stages, key, self.lane)
                .await
                .map_err(|(e, _)| e)?;
        }
        Err(copy_text("beUses.session.exhausted", &[]).into())
    }
}

/// 链路字节之外的那几样（今天只有改 PTY 窗口大小：`(列, 行)`）。没有 ⇒ [`Control::none`]。
#[derive(Default)]
pub struct Control {
    pub resize: Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u32)>>,
}

impl Control {
    pub fn none() -> Self {
        Control::default()
    }
}

/// 一份拨号请求的完整一趟：拿连接（池里复用或新拨）→ 按用法服务 → 结束（调用方手里那根下行管子读到 EOF）。
/// 返回类型显式写出 `+ Send`：理由同 [`exec`]（russh 借 `&self` 的 `async fn` 在高阶生命周期上推不出 Send）。
pub fn run<'a, R, W>(
    req: &'a DialRequest,
    stages: &'a StageSink,
    input: R,
    out: &'a mut W,
) -> impl std::future::Future<Output = ()> + Send + 'a
where
    R: AsyncRead + Unpin + Send + 'a,
    W: AsyncWrite + Unpin + Send,
{
    run_with(req, stages, input, out, Control::none())
}

/// 同 [`run`]，另带链路字节之外的那几样（[`Control`]：PTY 改窗口）。
pub fn run_with<'a, R, W>(
    req: &'a DialRequest,
    stages: &'a StageSink,
    input: R,
    out: &'a mut W,
    control: Control,
) -> impl std::future::Future<Output = ()> + Send + 'a
where
    R: AsyncRead + Unpin + Send + 'a,
    W: AsyncWrite + Unpin + Send,
{
    async move {
        // 要进展行 ⇒ 这一趟的阶段出口另接一头，ack 之前拨号那几段边等边把报来的那一步当场写出去（[`moving`]）。
        let (live, mut steps) = if req.progress {
            let (s, rx) = stages.reporting();
            (s, Some(rx))
        } else {
            (stages.clone(), None)
        };
        let stages = &live;
        let taken = moving(out, &mut steps, Lease::take(req, stages, lane_of(req.use_))).await;
        let mut lease = match taken {
            Ok(l) => l,
            Err((e, fp)) => {
                tracing::error!("dial: 拨号失败: {e}");
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(e, fp).because(stages.why()).seen(stages),
                )
                .await;
                return;
            }
        };
        serve(req, &mut lease, stages, &mut steps, input, out, control).await;
    }
}

/// ack 之前的一段（拿连接 · 开通道）：等 `fut` 的同时把报来的进展（[`StageSink::step`]）当场写成 `{"progress":…}` 行；
/// `fut` 好了之后把已经报了、还没写的那几步补完再回（之后就是 ack）。不要进展行（`steps` 为 `None`）⇒ 就是等它。
/// 写不出去（对面走了）⇒ 不再写，照等 `fut`（收场由写 ack 那一下发现）。
async fn moving<W, F>(
    out: &mut W,
    steps: &mut Option<tokio::sync::mpsc::UnboundedReceiver<&'static str>>,
    fut: F,
) -> F::Output
where
    W: AsyncWrite + Unpin + Send,
    F: std::future::Future,
{
    tokio::pin!(fut);
    loop {
        let Some(rx) = steps.as_mut() else {
            return fut.await;
        };
        tokio::select! {
            biased;
            r = &mut fut => {
                while let Ok(st) = rx.try_recv() {
                    if write_line(out, &serde_json::json!({ "progress": st })).await.is_err() {
                        break;
                    }
                }
                return r;
            }
            got = rx.recv() => {
                let wrote = match got {
                    Some(st) => write_line(out, &serde_json::json!({ "progress": st })).await.is_ok(),
                    None => false,
                };
                if !wrote {
                    *steps = None;
                }
            }
        }
    }
}

/// 拨通之后的那一段。
async fn serve<R, W>(
    req: &DialRequest,
    lease: &mut Lease,
    stages: &StageSink,
    steps: &mut Option<tokio::sync::mpsc::UnboundedReceiver<&'static str>>,
    mut input: R,
    out: &mut W,
    control: Control,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    match req.use_ {
        Use::Stream => {
            // `_permit`：这条长流占着这条连接的一格通道，直到本臂返回（`pool::Budget`）。
            let (channel, _permit) =
                match moving(out, steps, lease.session_channel(req, stages)).await {
                    Ok(c) => c,
                    Err(e) => {
                        let fp = lease.linked.fingerprint.clone();
                        let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                        return;
                    }
                };
            let fail = |e: Said| DialAck::failed(e, lease.linked.fingerprint.clone());
            // want_reply = true：等远端确认 exec 成功再回 ack。
            let opened = exec(&channel, req.command.as_bytes().to_vec())
                .await
                .map_err(|e| Said::with_raw(copy_text("beUses.exec.failed", &[]), &e));
            if let Err(e) = opened {
                let _ = write_stages_then_ack(out, stages, &fail(e)).await;
                return;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                tracing::error!("dial: 写 ack 失败（界面已经走了？）");
                return;
            }
            // 两条方向对拷，哪一边先结束就收工：下行结束 = 远端那头走了；上行结束 = 界面走了（链路被关 / 句柄被丢）。
            // 不许把上行 EOF 读成「半关、接着等下行」：远端后端的长流不会因为 stdin EOF 退出，会挂在一条没人收的下行上。
            let (mut down, mut up) = tokio::io::split(channel.into_stream());
            tokio::select! {
                r = tokio::io::copy(&mut input, &mut up) => {
                    tracing::info!("dial: 上行结束（界面那头断了）：{r:?}");
                }
                r = tokio::io::copy(&mut down, out) => {
                    tracing::info!("dial: 下行结束（远端那头断了）：{r:?}");
                }
            }
        }
        Use::Capture => {
            let fp = lease.linked.fingerprint.clone();
            let Some(opts) = req.capture.clone() else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(crate::malformed("use=capture without `capture`").into(), fp),
                )
                .await;
                return;
            };
            let (channel, _permit) =
                match moving(out, steps, lease.session_channel(req, stages)).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                        return;
                    }
                };
            // 读半边收全，写半边被守着：本臂无论怎么收场（收全 · 提前收工 · 链路被关 ⇒ abort），远端那条通道都被关。
            let (mut rd, wr) = channel.split();
            let wr = CloseOnDrop::new(wr);
            if let Err(e) = exec_half(wr.get(), req.command.as_bytes().to_vec()).await {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(Said::with_raw(copy_text("beUses.exec.failed", &[]), &e), fp),
                )
                .await;
                return;
            }
            // 有载荷就写进远端进程的 stdin（不关 —— 收的一侧只读一行）。
            if let Some(input) = opts.stdin.clone() {
                if let Err(e) = wr.get().data_bytes(input.into_bytes()).await {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &DialAck::failed(
                            Said::with_raw(copy_text("beUses.exec.stdinLost", &[]), &e),
                            fp,
                        ),
                    )
                    .await;
                    return;
                }
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                return;
            }
            let got = collect(&mut rd, &opts).await;
            let _ = write_line(out, &got).await;
        }
        Use::Forward => {
            let fp = lease.linked.fingerprint.clone();
            let Some(spec) = req.forward.clone() else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(crate::malformed("use=forward without `forward`").into(), fp),
                )
                .await;
                return;
            };
            // 只绑回环（不对外暴露）。绑口是**后端**的事。
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", spec.local_port)).await
            {
                Ok(l) => l,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &DialAck::failed(
                            Said::with_raw(
                                copy_text(
                                    "beUses.forward.bindFailed",
                                    &[
                                        ("port", &spec.local_port.to_string()),
                                        ("why", &copy_core::io_reason(e.kind())),
                                    ],
                                ),
                                &e,
                            ),
                            fp,
                        ),
                    )
                    .await;
                    return;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                return;
            }
            forward(Arc::clone(&lease.linked), listener, spec, &mut input, out).await
        }
        Use::Hold => {
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                return;
            }
            // 上行读到 EOF（对面走了）或连接没了 ⇒ 收工；上行的字节不是协议的一部分，读到就扔。
            let heard = Arc::clone(&lease.linked.heard);
            let mut sink = [0u8; 64];
            loop {
                tokio::select! {
                    r = input.read(&mut sink) => if matches!(r, Ok(0) | Err(_)) { break },
                    () = heard.gone() => {
                        tracing::info!("dial: 握着的那条连接没了，这条链路收工");
                        break;
                    }
                }
            }
        }
        Use::Shell => {
            let fp = lease.linked.fingerprint.clone();
            let pty = req.pty.clone().unwrap_or_default();
            let (channel, _permit) =
                match moving(out, steps, lease.session_channel(req, stages)).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                        return;
                    }
                };
            // 读写两半分开：写半边被守着（无论怎么收场远端那条通道都被关），改窗口也走它。
            let (mut rd, wr) = channel.split();
            let wr = CloseOnDrop::new(wr);
            let opened = async {
                pty_half(wr.get(), &pty).await?;
                shell_half(wr.get()).await
            }
            .await
            .map_err(|e| Said::with_raw(copy_text("beUses.shell.failed", &[]), &e));
            if let Err(e) = opened {
                let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                return;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                return;
            }
            let mut up = wr.get().make_writer();
            let mut down = rd.make_reader();
            let up_done = tokio::io::copy(&mut input, &mut up);
            let down_done = tokio::io::copy(&mut down, out);
            tokio::pin!(up_done, down_done);
            let mut resize = control.resize;
            loop {
                tokio::select! {
                    r = &mut up_done => {
                        tracing::info!("dial: shell 上行结束（对面断了）：{r:?}");
                        break;
                    }
                    r = &mut down_done => {
                        tracing::info!("dial: shell 下行结束（远端那头断了）：{r:?}");
                        break;
                    }
                    size = recv_resize(&mut resize) => match size {
                        Some((cols, rows)) => {
                            if let Err(e) = window_half(wr.get(), cols, rows).await {
                                tracing::info!("dial: 改窗口没成：{e}");
                            }
                        }
                        // 改窗口那根管子没人了：之后只对拷字节。
                        None => resize = None,
                    },
                }
            }
        }
    }
}

/// 改窗口那根管子的下一条；没有管子 ⇒ 永远不到（`select!` 那一臂就此不动）。
async fn recv_resize(
    rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<(u32, u32)>>,
) -> Option<(u32, u32)> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

/// 要 PTY（显式 `Send` 的盒子，理由同 [`exec`]）。
fn pty_half<'a>(
    w: &'a russh::ChannelWriteHalf<russh::client::Msg>,
    pty: &'a crate::PtyOpts,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + 'a>> {
    Box::pin(w.request_pty(true, &pty.term, pty.cols, pty.rows, 0, 0, &[]))
}

/// 开 shell（同上）。
fn shell_half(
    w: &russh::ChannelWriteHalf<russh::client::Msg>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(w.request_shell(true))
}

/// 改窗口（同上）。
fn window_half(
    w: &russh::ChannelWriteHalf<russh::client::Msg>,
    cols: u32,
    rows: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(w.window_change(cols, rows, 0, 0))
}

/// 收全 stdout / stderr / 退出码。EOF 之后服务端才送 exit-status ⇒ **不能见 Eof 就收**，
/// 收到 Close / 通道关闭才算完。`abort_marker` 一出现就提前收（老后端掉进流模式永不 EOF）。
async fn collect(channel: &mut russh::ChannelReadHalf, opts: &CaptureOpts) -> Captured {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let mut status: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                if out.len() < opts.max_bytes {
                    out.extend_from_slice(&data);
                }
                if let Some(marker) = opts.abort_marker.as_deref() {
                    if String::from_utf8_lossy(&out).contains(marker) {
                        break;
                    }
                }
            }
            russh::ChannelMsg::ExtendedData { data, .. } => {
                if err.len() < opts.max_bytes {
                    err.extend_from_slice(&data);
                }
            }
            russh::ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }
    Captured {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
        exit_status: status,
    }
}

/// 端口转发：每接进一条连接开一条 direct-tcpip、双向对拷，并往下行报一行 `{"accepted":n}`。
/// **界面走了（上行 EOF / 链路被关）就收工**：丢掉 listener（本地口释放）＋ 收掉在飞的隧道。
///
/// 连接是池里的、别的链路也在用 ⇒ **不再 `disconnect`**（那会把同一台远端的长流一起掐断）；
/// 在飞的隧道改由本函数自己的 `JoinSet` 收掉（它随本函数返回被丢掉 ⇒ 全部 abort）。
async fn forward<R, W>(
    linked: Arc<Linked>,
    listener: tokio::net::TcpListener,
    spec: crate::ForwardSpec,
    input: &mut R,
    out: &mut W,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    let mut accepted: u64 = 0;
    let mut tunnels: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
    let mut sink = [0u8; 64];
    loop {
        tokio::select! {
            r = input.read(&mut sink) => {
                // 界面那头断了（EOF / 错误）⇒ 收工。读到字节不是协议的一部分，照样当作「还活着」。
                if matches!(r, Ok(0) | Err(_)) {
                    break;
                }
            }
            a = listener.accept() => {
                let (mut tcp, _peer) = match a {
                    Ok(v) => v,
                    // 不退避、不重试：本 crate 不许「睡到点自己醒」，不退避直接重试会在 fd 耗尽（EMFILE，持续性的）时空转吃满一核
                    // ⇒ 收工并出声：界面读到链路结束，把这条转发标成 `error`，用户重开即可。代价：一次真·瞬时的 ECONNABORTED 也会让这条转发结束。
                    Err(e) => {
                        tracing::error!("dial: 转发的本地口 accept 失败，这条转发收工：{e}");
                        break;
                    }
                };
                accepted += 1;
                if write_line(out, &serde_json::json!({ "accepted": accepted })).await.is_err() {
                    break;
                }
                let linked = Arc::clone(&linked);
                let host = spec.remote_host.clone();
                let port = u32::from(spec.remote_port);
                tunnels.spawn(async move {
                    if let Ok(channel) = linked
                        .session
                        .channel_open_direct_tcpip(host, port, "127.0.0.1".to_string(), 0)
                        .await
                    {
                        let mut chs = channel.into_stream();
                        if let Err(e) = tokio::io::copy_bidirectional(&mut tcp, &mut chs).await {
                            tracing::info!("dial: 一条转发连接结束：{e}");
                        }
                    }
                });
            }
        }
    }
    drop(listener);
    tunnels.abort_all();
    let _ = out.flush().await;
}
