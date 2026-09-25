//! 开出 channel 之后的三种用法（[`super::Use`]）。握手已过：每一支先回 ack（失败就回失败的 ack），
//! 再干自己那件事。
//!
//! 〔SR1a〕I/O 从「本进程的 stdin / stdout」换成调用方给的一对 `AsyncRead` / `AsyncWrite`
//! （常驻后端里是那条链路的两根内存管子，`link.rs`）；连接从「这一趟自己拨的」换成
//! 「池里拿的」（[`Lease`]，同身份复用）。三种用法本身一行语义没改。

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::connect::{self, Linked};
use super::{
    pool, write_line, write_stages_then_ack, CaptureOpts, Captured, DialAck, DialRequest,
    StageSink, Use, ACK_V, USES,
};

fn ok_ack(l: &Linked) -> DialAck {
    DialAck {
        ok: true,
        error: None,
        fingerprint: l.fingerprint.clone(),
        endpoint: Some(l.endpoint.clone()),
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

/// 这一趟手里的那条 SSH 连接：池里复用的，或者这一趟新拨的。
pub(crate) struct Lease {
    /// 池里的身份；`None` = 不进池（测试连接那一形：它要看的就是一次真拨号的阶段）。
    key: Option<String>,
    linked: Arc<Linked>,
    /// 这条是复用来的（不是这一趟拨的）。只有复用来的，开 channel 失败才值得重拨一次。
    reused: bool,
}

impl Lease {
    /// 拿一条连接。`probe` / `stages` ⇒ 不进池、就地拨；其余按身份复用。
    pub(crate) async fn take(
        req: &DialRequest,
        stages: &StageSink,
    ) -> Result<Lease, (String, Option<String>)> {
        if req.probe || req.stages {
            let linked = connect::establish(req, stages).await?;
            return Ok(Lease {
                key: None,
                linked: Arc::new(linked),
                reused: false,
            });
        }
        let key = pool::identity(req);
        let (linked, reused) = pool::ssh()
            .get(&key, || connect::establish(req, stages))
            .await?;
        tracing::info!(
            "dial: {} {} 的那条 SSH 连接（池里此刻 {} 条）",
            if reused { "复用" } else { "新拨了" },
            linked.endpoint,
            pool::ssh().live()
        );
        Ok(Lease {
            key: Some(key),
            linked,
            reused,
        })
    }

    /// 这一趟手里那条连接（调用方要把它攥到通道用完为止 —— 句柄一 drop 整条连接就断）。
    pub(crate) fn linked(&self) -> &Arc<Linked> {
        &self.linked
    }

    /// 开一条 session channel，**先过这条连接的通道预算**（〔SR1b〕`pool::Budget`：长流 · 查询 · SFTP
    /// 同一条连接、同一道闸；`lane` = 这一格是不是传输）。借到的那一格随返回的 [`pool::Permit`] 走，
    /// 调用方攥到通道用完为止。
    ///
    /// **复用来的连接上开失败 ⇒ 从池里摘掉、重拨一次**（同一机制换一条新连接；新拨的那条再失败就如实报）。
    /// 重拨之后预算按**新那条**连接记（旧那一格随失败一起还掉）。
    pub(crate) async fn session_channel(
        &mut self,
        req: &DialRequest,
        stages: &StageSink,
        lane: Lane,
    ) -> Result<(russh::Channel<russh::client::Msg>, pool::Permit), String> {
        let permit = lane.take(&self.linked).await?;
        match self.linked.session.channel_open_session().await {
            Ok(c) => Ok((c, permit)),
            Err(e) => {
                drop(permit);
                let Some(key) = self.key.clone().filter(|_| self.reused) else {
                    return Err(format!("打开 session channel 失败: {e}"));
                };
                tracing::warn!("dial: 复用的连接上开 channel 失败（{e}）—— 摘掉它、重拨一次");
                pool::ssh().evict(&key, &self.linked);
                let (linked, _) = pool::ssh()
                    .get(&key, || connect::establish(req, stages))
                    .await
                    .map_err(|(e, _)| e)?;
                self.linked = linked;
                self.reused = false;
                let permit = lane.take(&self.linked).await?;
                let c = self
                    .linked
                    .session
                    .channel_open_session()
                    .await
                    .map_err(|e| format!("打开 session channel 失败: {e}"))?;
                Ok((c, permit))
            }
        }
    }
}

/// 一条 session 通道占哪一种格（`pool::Budget` 的两道闸）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// 长流 · capture · files 链路：只过通道闸。
    Session,
    /// 传输：先过传输车道、再过通道闸。
    Transfer,
}

impl Lane {
    async fn take(self, linked: &Linked) -> Result<pool::Permit, String> {
        let p = match self {
            Lane::Session => linked.budget.session().await,
            Lane::Transfer => linked.budget.transfer().await,
        }?;
        let (s, t) = linked.budget.free();
        tracing::debug!(
            "dial: {} 上借了一格{}（通道还剩 {s}、传输车道还剩 {t}）",
            linked.endpoint,
            if self == Lane::Transfer { "传输" } else { "" }
        );
        Ok(p)
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
        let mut lease = match Lease::take(req, stages).await {
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
            let (channel, _permit) = match lease.session_channel(req, stages, Lane::Session).await {
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
                .map_err(|e| format!("exec {} 失败: {e}", req.command));
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
                    &DialAck::failed("请求里 use=capture 却没给 capture 参数".into(), fp),
                )
                .await;
                return;
            };
            let (mut channel, _permit) =
                match lease.session_channel(req, stages, Lane::Session).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                        return;
                    }
                };
            if let Err(e) = exec(&channel, req.command.as_bytes().to_vec()).await {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed(format!("exec {} 失败: {e}", req.command), fp),
                )
                .await;
                return;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
                .await
                .is_err()
            {
                return;
            }
            let got = collect(&mut channel, &opts).await;
            let _ = write_line(out, &got).await;
        }
        Use::Forward => {
            let fp = lease.linked.fingerprint.clone();
            let Some(spec) = req.forward.clone() else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &DialAck::failed("请求里 use=forward 却没给 forward 参数".into(), fp),
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
                            format!("绑定本地端口 127.0.0.1:{} 失败: {e}", spec.local_port),
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
        Use::Files => {
            // 〔SR1b〕sftp 子系统开好了才回 ack：「远端没开 sftp」要落在 ack 那一行里，不是第一条应答里。
            let fp = lease.linked.fingerprint.clone();
            let session = match super::sftp::open(lease, req, stages, Lane::Session).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = write_stages_then_ack(out, stages, &DialAck::failed(e, fp)).await;
                    return;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&lease.linked))
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
async fn collect(channel: &mut russh::Channel<russh::client::Msg>, opts: &CaptureOpts) -> Captured {
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
