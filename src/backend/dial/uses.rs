//! 开出 channel 之后的三种用法（[`super::Use`]）。握手已过：每一支先回 ack（失败就回失败的 ack），
//! 再干自己那件事。

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::connect::Linked;
use super::{
    write_line, write_stages_then_ack, CaptureOpts, Captured, DialAck, DialRequest, StageSink, Use,
    ACK_V, EXIT_BAD_REQUEST, EXIT_DIAL_FAILED, USES,
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

/// 拨通之后的那一段。返回进程退出码。
pub(crate) async fn serve<W: tokio::io::AsyncWrite + Unpin>(
    req: &DialRequest,
    linked: Linked,
    stages: &StageSink,
    out: &mut W,
) -> i32 {
    let fail = |e: String| DialAck::failed(e, linked.fingerprint.clone());
    match req.use_ {
        Use::Stream => {
            let channel = match linked.session.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &fail(format!("打开 session channel 失败: {e}")),
                    )
                    .await;
                    return EXIT_DIAL_FAILED;
                }
            };
            // want_reply = true：等远端确认 exec 成功再回 ack。
            let opened = channel
                .exec(true, req.command.as_bytes())
                .await
                .map_err(|e| format!("exec {} 失败: {e}", req.command));
            if let Err(e) = opened {
                let _ = write_stages_then_ack(out, stages, &fail(e)).await;
                return EXIT_DIAL_FAILED;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&linked))
                .await
                .is_err()
            {
                tracing::error!("dial: 写 ack 失败（界面已经走了？）");
                return EXIT_DIAL_FAILED;
            }
            // 两条方向对拷。**下行结束就收工**（= 远端那头走了）；上行结束（界面关了写半边）只是
            // 把 EOF 递给远端（半关），远端命令读到 EOF 自己收尾之后下行才结束 —— 否则
            // 「界面写完就关」的那一形（一次性命令喂 stdin）会把还没回来的输出截掉。
            // 界面整个走了（管子断）⇒ 我们写 stdout 失败 ⇒ 下行那一边也结束。
            let (mut down, mut up) = tokio::io::split(channel.into_stream());
            let upstream = tokio::spawn(async move {
                let mut input = tokio::io::stdin();
                let r = tokio::io::copy(&mut input, &mut up).await;
                tracing::info!("dial: 上行结束（界面关了写半边）：{r:?}");
                if let Err(e) = up.shutdown().await {
                    tracing::info!("dial: 把 EOF 递给远端失败：{e}");
                }
            });
            let r = tokio::io::copy(&mut down, out).await;
            tracing::info!("dial: 下行结束（远端那头走了 / 界面不收了）：{r:?}");
            upstream.abort();
            0
        }
        Use::Capture => {
            let Some(opts) = req.capture.clone() else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &fail("请求里 use=capture 却没给 capture 参数".into()),
                )
                .await;
                return EXIT_BAD_REQUEST;
            };
            let mut channel = match linked.session.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &fail(format!("打开 session channel 失败: {e}")),
                    )
                    .await;
                    return EXIT_DIAL_FAILED;
                }
            };
            if let Err(e) = channel.exec(true, req.command.as_bytes()).await {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &fail(format!("exec {} 失败: {e}", req.command)),
                )
                .await;
                return EXIT_DIAL_FAILED;
            }
            if write_stages_then_ack(out, stages, &ok_ack(&linked))
                .await
                .is_err()
            {
                return EXIT_DIAL_FAILED;
            }
            let got = collect(&mut channel, &opts).await;
            if write_line(out, &got).await.is_err() {
                return EXIT_DIAL_FAILED;
            }
            0
        }
        Use::Forward => {
            let Some(spec) = req.forward.clone() else {
                let _ = write_stages_then_ack(
                    out,
                    stages,
                    &fail("请求里 use=forward 却没给 forward 参数".into()),
                )
                .await;
                return EXIT_BAD_REQUEST;
            };
            // 只绑回环（不对外暴露）。绑口是**后端**的事（`设计/01 §2.1 C5`）。
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", spec.local_port)).await
            {
                Ok(l) => l,
                Err(e) => {
                    let _ = write_stages_then_ack(
                        out,
                        stages,
                        &fail(format!(
                            "绑定本地端口 127.0.0.1:{} 失败: {e}",
                            spec.local_port
                        )),
                    )
                    .await;
                    return EXIT_DIAL_FAILED;
                }
            };
            if write_stages_then_ack(out, stages, &ok_ack(&linked))
                .await
                .is_err()
            {
                return EXIT_DIAL_FAILED;
            }
            forward(linked, listener, spec, out).await
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

/// 端口转发：每接进一条连接开一条 direct-tcpip、双向对拷，并往 stdout 报一行 `{"accepted":n}`。
/// **界面走了（stdin EOF）就收工**：丢掉 listener（本地口释放）＋ 主动断开 SSH（在飞的隧道随之死）。
async fn forward<W: tokio::io::AsyncWrite + Unpin>(
    linked: Linked,
    listener: tokio::net::TcpListener,
    spec: super::ForwardSpec,
    out: &mut W,
) -> i32 {
    let session = std::sync::Arc::new(linked.session);
    let mut accepted: u64 = 0;
    let mut input = tokio::io::stdin();
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
                    //   ⇒ **收工并出声**：界面那侧读到管子关了，把这条转发标成 `error`，用户重开即可。
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
                let session = std::sync::Arc::clone(&session);
                let host = spec.remote_host.clone();
                let port = u32::from(spec.remote_port);
                tokio::spawn(async move {
                    if let Ok(channel) = session
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
    if let Err(e) = session
        .disconnect(
            russh::Disconnect::ByApplication,
            "cc-monitor: stop forward",
            "",
        )
        .await
    {
        tracing::info!("dial: 断开转发连接：{e}");
    }
    let _ = out.flush().await;
    0
}
