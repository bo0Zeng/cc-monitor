//! 开出 channel 之后的三种用法（[`super::Use`]）。握手已过：每一支先回 ack（失败就回失败的 ack），
//! 再干自己那件事。
//!
//! 〔SR1a〕I/O 从「本进程的 stdin / stdout」换成调用方给的一对 `AsyncRead` / `AsyncWrite`
//! （常驻后端里是那条链路的两根内存管子，`link.rs`）；连接从「这一趟自己拨的」换成
//! 「池里拿的」（[`Lease`]，同身份复用）。三种用法本身一行语义没改。

use copy_core::copy_text;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::connect::{self, Linked};
use super::{
    pool, write_line, write_stages_then_ack, CaptureOpts, Captured, DialAck, DialRequest,
    StageSink, Use, ACK_V, USES,
};

/// 〔MIG-1 收尾〕这一趟目标那台 / 跳板那台是不是严格校验指纹（请求里带了非空的期望指纹）—— ack 的 `strict` / `jump_strict`。
pub(crate) fn strictness(req: &DialRequest) -> (bool, bool) {
    let set = |f: &Option<String>| f.as_deref().is_some_and(|s| !s.trim().is_empty());
    (
        set(&req.host_key_fingerprint),
        req.jump
            .as_ref()
            .is_some_and(|j| set(&j.host_key_fingerprint)),
    )
}

fn ok_ack(l: &Linked, req: &DialRequest) -> DialAck {
    let (strict, jump_strict) = strictness(req);
    DialAck {
        ok: true,
        error: None,
        fingerprint: l.fingerprint.clone(),
        fingerprints: l.fingerprints.clone(),
        jump_fingerprints: l.jump_fingerprints.clone(),
        endpoint: Some(l.endpoint.clone()),
        winner: Some(l.winner.clone()),
        strict,
        jump_strict,
        v: ACK_V,
        uses: USES,
    }
}

/// `channel.exec(true, cmd)` 装进一个**显式 `Send`** 的盒子。
///
/// ⚠ 不是装饰：russh 的 `Channel::exec` 是借 `&self` 的 `async fn`，放进要 `tokio::spawn` 的链路任务里
/// 会撞 rustc 的「`implementation of Send is not general enough`」（Send 在高阶生命周期上推不出来）。
/// 在这里把 Send 写死在一个具体生命周期上，调用点就不必再推。
fn exec(
    channel: &russh::Channel<russh::client::Msg>,
    cmd: Vec<u8>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(channel.exec(true, cmd))
}

/// 〔NT2 · A4〕同上，但给**写半边**（capture 那一臂先 `split` 再 exec：读半边交给 [`collect`]，写半边交给 [`CloseOnDrop`]）。
fn exec_half(
    w: &russh::ChannelWriteHalf<russh::client::Msg>,
    cmd: Vec<u8>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), russh::Error>> + Send + '_>> {
    Box::pin(w.exec(true, cmd))
}

/// 〔NT2 · A4〕能被「关掉」的那一半（生产 = russh 的 `ChannelWriteHalf`；判据用记账替身）。
pub(crate) trait Closable: Send + 'static {
    fn close_it(self) -> impl std::future::Future<Output = ()> + Send;
}

impl Closable for russh::ChannelWriteHalf<russh::client::Msg> {
    fn close_it(self) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let _ = self.close().await;
        }
    }
}

/// 〔NT2 · A4〕**被丢 ⇒ 向远端发一次关通道**（与 russh 自己给 `into_stream` 那一形的 `ChannelCloseOnDrop` 同形）。
///
/// 为什么要它：russh 0.61 的裸 `Channel` 被丢**不发 `CHANNEL_CLOSE`**（只有 `into_stream` 那一形会发）。
/// capture 那一臂拿的是裸通道 ⇒ 链路被关（调用方期限到点 / 界面走了）或 `abort_marker` 提前收工（老后端掉进流模式）时，
/// 本地那一格已经还回预算，远端那条 session 通道与它上面的进程却还开着 ⇒ 下一次开通道被远端回拒、这条连接的上限被**学小**。
/// 关通道是**尽力**的（`设计/05 §3.3.3`：对端撤活只是尽力）：发出去了，对面怎么收场是它的事。
pub(crate) struct CloseOnDrop<W: Closable>(Option<W>);

impl<W: Closable> CloseOnDrop<W> {
    pub(crate) fn new(w: W) -> Self {
        CloseOnDrop(Some(w))
    }

    pub(crate) fn get(&self) -> &W {
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

/// 一条 session 通道（或一条隧道）走哪一道 —— 〔NT1〕住 `pool.rs`（放置要按它判），这里转一手。
pub(crate) use super::pool::Lane;

/// 一份拨号请求的用法 ⇒ 它在池里走哪一道。
pub(crate) fn lane_of(u: Use) -> Lane {
    match u {
        Use::Stream => Lane::Stream,
        Use::Capture | Use::Files => Lane::Query,
        Use::Forward | Use::Tunnel => Lane::Tunnel,
    }
}

/// 这一趟手里的那条 SSH 连接：池里放置来的（复用 / 新拨 / 多开），或者这一趟就地拨的（测试连接）。
pub(crate) struct Lease {
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

impl Lease {
    /// 拿一条连接。`probe` / `stages` ⇒ 不进池、就地拨；其余按身份在池里放置（`pool::Pool::place`：复用 / 按需多开）。
    pub(crate) async fn take(
        req: &DialRequest,
        stages: &StageSink,
        lane: Lane,
    ) -> Result<Lease, (String, Option<String>)> {
        if req.probe || req.stages {
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
    ) -> Result<Lease, (String, Option<String>)> {
        let placed = pool::ssh()
            .place(&key, lane, || connect::establish(req, stages))
            .await?;
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
    pub(crate) fn linked(&self) -> &Arc<Linked> {
        &self.linked
    }

    /// 这一趟走哪一道（`sftp.rs` 据它判「用完停不停进空位」：只有传输停）。
    pub(crate) fn lane(&self) -> Lane {
        self.lane
    }

    /// 开一条 session channel。那一格**放置时已经借好了**（`pool::Budget`：长流 · 查询 · SFTP 同一条连接、同一道闸），
    /// 随返回的 [`pool::Permit`] 走，调用方攥到通道用完为止。
    ///
    /// 开失败分两形：
    /// - **远端说满了**（`MaxSessions`）⇒ 〔NT1〕这条连接学到上限（`Budget::refused`：空格当场作废）、**不摘它**
    ///   （长流还在它上面），照同一道重新放置 —— 通常落到一条新连接上（`Why::Full`）；学到的上限是 0 ⇒ 报错（远端根本不给开）。
    /// - **连接死了**（其余）⇒ 复用来的才摘掉、重拨一次（同一机制换一条新连接）；新拨的那条再失败就如实报。
    pub(crate) async fn session_channel(
        &mut self,
        req: &DialRequest,
        stages: &StageSink,
    ) -> Result<(russh::Channel<russh::client::Msg>, pool::Permit), String> {
        for _ in 0..=pool::MAX_CONNECTIONS_PER_HOST {
            let Some(permit) = self.permit.take() else {
                return Err(copy_text("beUses.session.noSlot", &[]));
            };
            // 〔NT1〕等远端回「开好了」这一段被打断（链路被关）⇒ 摘掉这条连接（`pool::Watch`）。
            let opened = {
                let watch = self
                    .key
                    .as_deref()
                    .map(|k| pool::Watch::new(pool::ssh(), k, &self.linked));
                let r = self.linked.session.channel_open_session().await;
                if let Some(w) = watch {
                    w.done();
                }
                r
            };
            let e = match opened {
                Ok(c) => return Ok((c, permit)),
                Err(e) => e,
            };
            drop(permit);
            let Some(key) = self.key.clone() else {
                return Err(copy_text(
                    "beUses.session.openFailed",
                    &[("e", &e.to_string())],
                ));
            };
            if refused_by_remote(&e) && !self.linked.session.is_closed() {
                let cap = self.linked.budget.refused();
                tracing::warn!(
                    "dial: 远端回拒了 {} 上的一条 session 通道（{e}）—— 这条连接学到上限 {cap}，换一格放",
                    self.linked.endpoint
                );
                if cap == 0 {
                    return Err(copy_text(
                        "beUses.session.refusedAll",
                        &[("e", &e.to_string())],
                    ));
                }
            } else if self.reused {
                tracing::warn!("dial: 复用的连接上开 channel 失败（{e}）—— 摘掉它、重拨一次");
                pool::ssh().evict(&key, &self.linked);
            } else {
                return Err(copy_text(
                    "beUses.session.openFailed",
                    &[("e", &e.to_string())],
                ));
            }
            *self = Self::place(req, stages, key, self.lane)
                .await
                .map_err(|(e, _)| e)?;
        }
        Err(copy_text("beUses.session.exhausted", &[]))
    }
}

/// 一份拨号请求的**完整一趟**：拿连接（池里复用或新拨）→ 按用法服务 → 结束。
/// 结束 = 调用方手里那根下行管子读到 EOF（`out` 随本函数返回被丢掉）。
///
/// ⚠ 返回类型**显式写出 `+ Send`**，不是装饰：链路任务要 `tokio::spawn`，而 `async fn` 的 Send 由调用点
/// 在高阶生命周期上推 —— russh 那几个借 `&self` 的 `async fn`（`exec` 等）会撞 rustc 的
/// 「`implementation of Send is not general enough`」。在定义处写死 Send，推导就落在具体生命周期上。
pub(crate) fn run<'a, R, W>(
    req: &'a DialRequest,
    stages: &'a StageSink,
    input: R,
    out: &'a mut W,
) -> impl std::future::Future<Output = ()> + Send + 'a
where
    R: AsyncRead + Unpin + Send + 'a,
    W: AsyncWrite + Unpin + Send,
{
    async move {
        let mut lease = match Lease::take(req, stages, lane_of(req.use_)).await {
            Ok(l) => l,
            Err((e, fp)) => {
                tracing::error!("dial: 拨号失败: {e}");
                let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                return;
            }
        };
        serve(req, &mut lease, stages, input, out).await;
    }
}

/// 拨通之后的那一段。
async fn serve<R, W>(
    req: &DialRequest,
    lease: &mut Lease,
    stages: &StageSink,
    mut input: R,
    out: &mut W,
) where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    match req.use_ {
        Use::Stream => {
            // `_permit`：这条长流占着这条连接的一格通道，直到本臂返回（`pool::Budget`）。
            let (channel, _permit) = match lease.session_channel(req, stages).await {
                Ok(c) => c,
                Err(e) => {
                    let fp = lease.linked.fingerprint.clone();
                    let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                    return;
                }
            };
            let fail = |e: String| DialAck::failed(e, lease.linked.fingerprint.clone());
            // want_reply = true：等远端确认 exec 成功再回 ack。
            let opened = exec(&channel, req.command.as_bytes().to_vec())
                .await
                .map_err(|e| copy_text("beUses.exec.failed", &[("e", &e.to_string())]));
            if let Err(e) = opened {
                let _ = write_stages_then_ack(out, stages, &fail(e)).await;
                return;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked, req))
                .await
                .is_err()
            {
                tracing::error!("dial: 写 ack 失败（界面已经走了？）");
                return;
            }
            // 两条方向对拷。**哪一边先结束就收工**（`K-P6b` 那一版的语义，C2 一度改成「上行结束只半关」又改回来）：
            // 下行结束 = 远端那头走了；上行结束 = **界面走了**（链路被关 / 句柄被丢）。
            // ⚠ 不许把上行 EOF 读成「半关、接着等下行」：远端后端的长流**不会**因为 stdin EOF 退出
            //   ⇒ 会挂在一条没人收的下行上（C2 现打逮到过一个这样挂了 42 分钟的代理）。
            //   本 crate 不许睡，也就没有「等一会儿再收」这一形。
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
                    &DialAck::failed(
                        crate::common::contract::malformed("use=capture without `capture`"),
                        fp,
                    ),
                )
                .await;
                return;
            };
            let (channel, _permit) = match lease.session_channel(req, stages).await {
                Ok(c) => c,
                Err(e) => {
                    let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                    return;
                }
            };
            // 〔NT2 · A4〕读半边收全，写半边被守着：本臂无论怎么收场（收全 · 提前收工 · 链路被关 ⇒ abort），远端那条通道都被关。
            let (mut rd, wr) = channel.split();
            let wr = CloseOnDrop::new(wr);
            if let Err(e) = exec_half(wr.get(), req.command.as_bytes().to_vec()).await {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(
                        copy_text("beUses.exec.failed", &[("e", &e.to_string())]),
                        fp,
                    ),
                )
                .await;
                return;
            }
            // 〔W5-AUX · `设计/96 §3.6`〕有载荷就写进远端进程的 stdin（不关 —— 收的一侧只读一行）。
            if let Some(input) = opts.stdin.clone() {
                if let Err(e) = wr.get().data_bytes(input.into_bytes()).await {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &DialAck::failed(
                            copy_text("beUses.exec.stdinLost", &[("e", &e.to_string())]),
                            fp,
                        ),
                    )
                    .await;
                    return;
                }
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked, req))
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
                    &DialAck::failed(
                        crate::common::contract::malformed("use=forward without `forward`"),
                        fp,
                    ),
                )
                .await;
                return;
            };
            // 只绑回环（不对外暴露）。绑口是**后端**的事（`设计/01 §2.1 C5`）。
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", spec.local_port)).await
            {
                Ok(l) => l,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &DialAck::failed(
                            copy_text(
                                "beUses.forward.bindFailed",
                                &[
                                    ("port", &spec.local_port.to_string()),
                                    ("e", &e.to_string()),
                                ],
                            ),
                            fp,
                        ),
                    )
                    .await;
                    return;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked, req))
                .await
                .is_err()
            {
                return;
            }
            forward(Arc::clone(&lease.linked), listener, spec, &mut input, out).await
        }
        Use::Tunnel => {
            let fp = lease.linked.fingerprint.clone();
            let Some(port) = req.tunnel_port else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(
                        crate::common::contract::malformed("use=tunnel without `tunnel_port`"),
                        fp,
                    ),
                )
                .await;
                return;
            };
            // 只到远端自己的回环（`listen::LOOPBACK` 那一格在远端）：连不上 = 那台上没人在听，落在 ack 里。
            let opened = lease
                .linked
                .session
                .channel_open_direct_tcpip("127.0.0.1", u32::from(port), "127.0.0.1", 0)
                .await;
            let channel = match opened {
                Ok(c) => c,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &DialAck::failed(
                            copy_text(
                                "beUses.tunnel.unreachable",
                                &[("port", &port.to_string()), ("e", &e.to_string())],
                            ),
                            fp,
                        ),
                    )
                    .await;
                    return;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked, req))
                .await
                .is_err()
            {
                return;
            }
            // 哪一边先结束就收工（与 `stream` 那一臂同一条理由）。
            let (mut down, mut up) = tokio::io::split(channel.into_stream());
            tokio::select! {
                r = tokio::io::copy(&mut input, &mut up) => {
                    tracing::info!("dial: 隧道上行结束（界面那头断了）：{r:?}");
                }
                r = tokio::io::copy(&mut down, out) => {
                    tracing::info!("dial: 隧道下行结束（远端那头断了）：{r:?}");
                }
            }
        }
        Use::Files => {
            // 〔SR1b〕sftp 子系统开好了才回 ack：「远端没开 sftp」要落在 ack 那一行里，不是第一条应答里。
            let fp = lease.linked.fingerprint.clone();
            let session = match super::sftp::open(lease, req, stages).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                    return;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked, req))
                .await
                .is_err()
            {
                return;
            }
            super::sftp::serve_files(&session, &mut input, out).await;
        }
    }
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
/// 〔SR1a〕连接是池里的、别的链路也在用 ⇒ **不再 `disconnect`**（那会把同一台远端的长流一起掐断）；
/// 在飞的隧道改由本函数自己的 `JoinSet` 收掉（它随本函数返回被丢掉 ⇒ 全部 abort）。
async fn forward<R, W>(
    linked: Arc<Linked>,
    listener: tokio::net::TcpListener,
    spec: super::ForwardSpec,
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
                    // ⚠ 界面侧原来在这里 100ms 退避后重试（瞬时错误不杀转发）。本 crate 不许「睡到点自己醒」
                    //   （`no_timer_guard`），而不退避直接重试会在 fd 耗尽（EMFILE，持续性的）时空转吃满一核
                    //   ⇒ **收工并出声**：界面那侧读到链路结束，把这条转发标成 `error`，用户重开即可。
                    //   代价：一次真·瞬时的 ECONNABORTED 也会让这条转发结束。
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
