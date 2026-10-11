//! **远端常驻后端，monitor 这一侧**（「远端常驻、本机远端同形」·）。
//!
//! 与本机宿主（`local_backend_host`）同形的四件：起 · 找 · 只升不降 · 停。设计住仓外。
//! - 起 · 找：经链路 `capture` 在远端跑 `--resident-ensure`（没人在听 ⇒ 起一个脱离的自己）→ 经链路 `stream`
//!   在远端 exec `--resident-attach`（**小中继**：连那台家里的套接字、原样双向对拷）→ 读 hello → 交 attach 行。
//!   门由 ssh 与那台的内核给：ssh 证明了是本人，中继在那台以本人身份连只给本人的套接字。没有钥匙。
//! - 只升不降：hello 的 build 比手上这一版旧 ⇒ `--resident-ensure --replace` 一次；比我新 ⇒ 照接。
//! 「换不换」由本机常驻后端判（帧命令 `resident-verdict`，与 `deploy-plan` 一家；判定只在后端），这里只照做。
//! - 停：`--resident-stop`（那台自己做「请它收尾 → 宽限期内等 → 到点强杀」，这里只发一次、拿回 `graceful | killed | not_running`）。
//!
//! 远端只有常驻这一形：那台答「脱离不了」（非 unix）⇒ 明说不支持；太旧不认这条子命令 ⇒ 出声报错。
//! 不回落到随 SSH 生死的流模式。

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::dial_host::DialStream;
use crate::stream_source::RemoteConfig;

/// 刚起的那一个还没绑上套接字：中继回 `absent` ⇒ 隔一会儿再接，封顶这么多次。
const RELAY_TRIES: u32 = 30;
const RELAY_WAIT: Duration = Duration::from_millis(200);

/// 不认 `--resident-ensure` 的老后端会把它当未知旗标、直接进流模式发 hello ⇒ 见到它就收工、当「太旧」。
const OLD_BACKEND_MARKER: &str = "\"kind\":\"hello\"";

/// 为什么没接上那台的常驻后端（那一句 ＋ 复制详情）。比较只比那一句与码（详情里有时刻）。
#[derive(Debug, Clone)]
pub(crate) enum AttachErr {
    /// 那台不是 Unix（后端脱离不了）⇒ **永久不支持**：记在那台的连接状态里，
    /// 不再自动按退避重连；界面出声，用户点「起」（`backend_start`）才再试一次。
    /// 第二格是那台状态成品里的原因码（`machine_state::NOT_UNIX`）。
    Unsupported(Said, &'static str),
    /// 别的失败 ⇒ 照常按退避重连。
    Failed(Said),
}

impl PartialEq for AttachErr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AttachErr::Unsupported(a, x), AttachErr::Unsupported(b, y)) => {
                a.said == b.said && x == y
            }
            (AttachErr::Failed(a), AttachErr::Failed(b)) => a.said == b.said,
            _ => false,
        }
    }
}

impl From<String> for AttachErr {
    fn from(s: String) -> Self {
        AttachErr::Failed(s.into())
    }
}

impl From<Said> for AttachErr {
    fn from(s: Said) -> Self {
        AttachErr::Failed(s)
    }
}

impl AttachErr {
    /// 给人看的那句 ＋ 复制详情。
    pub(crate) fn said(self) -> Said {
        match self {
            AttachErr::Unsupported(s, _) | AttachErr::Failed(s) => s,
        }
    }
}

/// 读 `--resident-ensure` / `--resident-stop` 那一趟的结果（纯函数）：退出 0 ⇒ stdout 那一行；退出 2 ⇒ stderr 的 `{code, message, detail, data?}`。
/// `unsupported`（那台脱离不了，非 unix）明说「远端只支持 Unix」；老后端 ⇒ 「太旧」—— 都是失败，没有回落。
/// `machine` 是那台给人看的称呼（老后端 ⇒「{machine} 后端要更新」）。
pub(crate) fn parse_answer(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<serde_json::Value, AttachErr> {
    if exec.stdout.contains(OLD_BACKEND_MARKER) {
        return Err(copy_core::backend_old(machine).into());
    }
    match exec.exit_status {
        Some(0) => serde_json::from_str(exec.stdout.trim()).map_err(|e| {
            Said::with_raw(
                copy_text("rsRemoteResident.ensure.answerUnreadable", &[]),
                &e,
            )
            .into()
        }),
        _ => {
            let err: serde_json::Value =
                serde_json::from_str(exec.stderr.trim()).unwrap_or_default();
            let msg = err["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| {
                    // 退出码只进日志；给人看的只说没起来、没说原因。
                    tracing::warn!(
                        "resident ensure exited {:?} without a reason",
                        exec.exit_status
                    );
                    copy_text("rsRemoteResident.ensure.noReason", &[])
                });
            // 那台的失败信封只有一种（`{code, message, detail, data?}`）：`detail` 是那台写好的一整份复制详情（下层原话在里面），
            //   原样当一块转交、补「本机」一行；老后端没写 ⇒ 只有那一句。
            let wrote = err["detail"].as_str().filter(|d| !d.trim().is_empty());
            if err["code"] == "unsupported" {
                // 那台的 `message` 这一格就是它的原话（脱离不了的系统报错）：句子由这边说，原话进详情。
                let said = copy_text("rsRemoteResident.ensure.unsupported", &[]);
                Err(AttachErr::Unsupported(
                    match wrote {
                        Some(d) => Said::relayed_from(said, d, Some(&msg)),
                        None => Said::with_raw(said, &msg),
                    },
                    crate::machine_state::NOT_UNIX,
                ))
            } else {
                Err(AttachErr::Failed(match wrote {
                    Some(d) => Said::relayed_from(msg, d, None),
                    None => msg.into(),
                }))
            }
        }
    }
}

/// `--resident-ensure` 的答：恰是一个对象（`{"pid":n|null}`）。别的形状 ⇒ 说不认得。
fn parse_ensured(v: &serde_json::Value) -> Result<(), String> {
    if v.is_object() {
        Ok(())
    } else {
        Err(copy_text("rsRemoteResident.ensure.answerIncomplete", &[]))
    }
}

async fn ensure(cfg: &RemoteConfig, replace: bool) -> Result<(), AttachErr> {
    // `ccm -- --resident-ensure`（打头的 `--` 让那台的 `ccm` 当后端用）。
    let mut cmd = format!(
        "{} {} --resident-ensure",
        crate::stream_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    if replace {
        cmd.push_str(" --replace");
    }
    let exec =
        crate::stream_source::connect_and_exec_capture(cfg, &cmd, Some(OLD_BACKEND_MARKER)).await?;
    Ok(parse_ensured(&parse_answer(&exec, &cfg.origin_label())?)?)
}

/// hello 那一行里那台报的 build（纯函数，只读线上形状）。第一行不是 hello ⇒ `Err`（那句话）。
/// 从前这里还判「换不换」（`hello_decision`〔散文墓碑〕调共享判定 `is_newer`，今天住后端 `control/deploy_plan.rs`）：判定进了本机常驻后端（[`ask_verdict`]）。
pub(crate) fn hello_build(line: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap_or_default();
    if v["kind"] != "hello" {
        return Err(copy_text("rsRemoteResident.hello.notOurs", &[]));
    }
    Ok(v["build_id"].as_str().unwrap_or_default().to_string())
}

/// 本机常驻后端那条命令的名字（与 `src/backend/stream/inbound/mod.rs::REGISTRY` 同名）。
pub(crate) const VERDICT_CMD: &str = "resident-verdict";

/// 问那一趟的上限：纯判定、不拨号，只是本机那条长连接上一问一答。
const VERDICT_BUDGET: Duration = Duration::from_secs(10);

/// 本机常驻后端对 hello 那一问的答：`replace` = 换掉再接 · `older` = 那台比手上这一版旧（版本那句话按它挑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Verdict {
    pub(crate) replace: bool,
    pub(crate) older: bool,
}

/// 应答 → [`Verdict`]（**严格收**：恰 `{action, older}` 两格、`action` 只认两个词 —— 两侧漂了当场说出来）。
pub(crate) fn decode_verdict(v: &serde_json::Value) -> Result<Verdict, Said> {
    let bad =
        |what: &str| Said::with_raw(copy_text("rsRemoteResident.verdict.unreadable", &[]), what);
    let obj = v.as_object().ok_or_else(|| bad("not an object"))?;
    if obj.len() != 2 {
        return Err(bad("expected exactly `action` and `older`"));
    }
    let replace = match obj.get("action").and_then(serde_json::Value::as_str) {
        Some("replace") => true,
        Some("attach") => false,
        _ => return Err(bad("`action` is neither `replace` nor `attach`")),
    };
    let older = obj
        .get("older")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| bad("`older` is not a bool"))?;
    Ok(Verdict { replace, older })
}

/// 换不换、旧不旧：「我这一版」（`mine`）有值才去问（`ask`，生产里是 [`ask_verdict`]）；手上没带后端字节（`None`）⇒
/// **不问**、直接「接、不判旧」—— 没有可放的字节，换装无从发起，新旧也无从比（版本那句话说「不可比」）。
pub(crate) async fn verdict_for<F, Fut>(mine: Option<&'static str>, ask: F) -> Result<Verdict, Said>
where
    F: FnOnce(&'static str) -> Fut,
    Fut: std::future::Future<Output = Result<Verdict, Said>>,
{
    match mine {
        Some(m) => ask(m).await,
        None => Ok(Verdict {
            replace: false,
            older: false,
        }),
    }
}

/// 问本机常驻后端「那台报 `theirs`，换还是接」。入参只有事实（手上这一版 · 那台报的 · 这一趟换过没有）。
async fn ask_verdict(mine: &str, theirs: &str, replaced: bool) -> Result<Verdict, Said> {
    use crate::backend_route::{route_call_error, Routed};
    let client = crate::dial_host::local_backend_accepting(VERDICT_CMD).await?;
    let args = serde_json::json!({ "mine": mine, "theirs": theirs, "replaced": replaced });
    let data = client
        .call(VERDICT_CMD, args, VERDICT_BUDGET)
        .await
        .map_err(|e| {
            match route_call_error(&e, &copy_core::local_machine(), |_code, message| {
                message.to_string()
            }) {
                Routed::NoChannel(s) | Routed::Refused(s) => Said::from(s),
            }
        })?;
    decode_verdict(&data.unwrap_or_default())
}

/// 远端这条流的出口声明：`session_added.pid` 不要（那台的 pid 只有本机 ↗ 绑窗口用得上）。
pub(crate) fn remote_stream_view() -> serde_json::Value {
    serde_json::json!({ "omit": { "session_added": ["pid"] } })
}

/// 读握手那一行（hello / attach 应答）；`hello` 选哪一句说「没答完」。
async fn read_line(r: &mut tokio::io::BufReader<DialStream>, hello: bool) -> Result<String, Said> {
    let mut buf = Vec::new();
    let got = crate::stream_source::read_capped_line(
        r,
        &mut buf,
        // hello / attach 应答那一行：与本机宿主同一个上限（同一条监听协议）。
        crate::local_backend_host::LISTEN_HANDSHAKE_LINE_CAP,
    )
    .await;
    match got {
        Ok(crate::stream_source::CappedLine::Line) => Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches(['\n', '\r'])
            .to_string()),
        Ok(_) if hello => Err(copy_text("rsRemoteResident.handshake.helloCut", &[]).into()),
        Ok(_) => Err(copy_text("rsRemoteResident.handshake.replyCut", &[]).into()),
        Err(e) => Err(Said::with_raw(
            copy_text("rsRemoteResident.handshake.readFailed", &[]),
            e,
        )),
    }
}

/// 接成的一条流：先吐出握手时读到的 hello 与预读的字节，再接着读链路 —— `stream_loop` 照读 stdio 流那样读它。
pub struct Replayed {
    head: std::io::Cursor<Vec<u8>>,
    inner: DialStream,
    /// 接上那一刻本机常驻后端答的「那台比手上这一版旧」（版本提示那句话按它挑，monitor 不自己比）。
    older: bool,
}

impl Replayed {
    /// 接上那一刻本机常驻后端答的「那台比手上这一版旧」。
    pub(crate) fn remote_is_older(&self) -> bool {
        self.older
    }
}

impl tokio::io::AsyncRead for Replayed {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = &mut *self;
        let pos = this.head.position() as usize;
        let rest = &this.head.get_ref()[pos..];
        if !rest.is_empty() {
            let n = rest.len().min(buf.remaining());
            buf.put_slice(&rest[..n]);
            this.head.set_position((pos + n) as u64);
            return std::task::Poll::Ready(Ok(()));
        }
        std::pin::Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Replayed {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// 中继回的第一行（纯函数）：`{"attach":"refused","reason":…}` ⇒ 那一格理由（`absent` 可等、别的不可等）；别的 ⇒ `None`（照 hello 读）。
pub(crate) fn relay_refusal(first: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(first.trim()).ok()?;
    (v["attach"] == "refused").then(|| v["reason"].as_str().unwrap_or_default().to_string())
}

/// 中继说「没人在听」的那个理由词（后端 `listen::REFUSE_ABSENT`，跨 crate 字面量，`the_relay_refusal_line_is_read_and_its_words_match_the_backend` 对拍）。
pub(crate) const RELAY_ABSENT: &str = "absent";

/// 接中继、读第一行：回「那条链路 ＋ 第一行」，或回一句「接不上」（`Ok(Err(reason))` 留给调用方判可不可以等）。
async fn relay_once(
    cfg: &RemoteConfig,
) -> Result<Result<(tokio::io::BufReader<DialStream>, String), String>, AttachErr> {
    let cmd = format!(
        "{} {} --resident-attach",
        crate::stream_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    let link = crate::dial_host::stream(cfg, &cmd).await?;
    let mut r = tokio::io::BufReader::new(link);
    let first = read_line(&mut r, true).await?;
    Ok(match relay_refusal(&first) {
        Some(reason) => Err(reason),
        None => Ok((r, first)),
    })
}

/// [`relay_once`] 的编排（`open` = 接一次中继；判据用替身）：`absent`（刚起的还没绑上）⇒ 隔 [`RELAY_WAIT`] 再接，
/// 至多 [`RELAY_TRIES`] 次，还是没有 ⇒ 「那台后端没在运行」；别的理由 ⇒ 当场停（再接也一样）。
pub(crate) async fn retry_relay<T, F, Fut>(
    mut open: F,
    machine: &str,
    tries: u32,
    wait: Duration,
) -> Result<T, AttachErr>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Result<T, String>, AttachErr>>,
{
    for _ in 0..tries {
        match open().await? {
            Ok(t) => return Ok(t),
            Err(reason) if reason == RELAY_ABSENT => {}
            Err(reason) => {
                return Err(Said::with_raw(
                    copy_text(
                        "rsRemoteResident.relay.unreachable",
                        &[("machine", machine)],
                    ),
                    reason,
                )
                .into())
            }
        }
        tokio::time::sleep(wait).await;
    }
    Err(Said::with_raw(
        copy_text("rsRemoteResident.relay.absent", &[("machine", machine)]),
        format!("{tries} × {}ms", wait.as_millis()),
    )
    .into())
}

/// **接上那台的常驻后端**（没有就起一个）：起 · 找 → 中继 → hello（旧 ⇒ 换一次）→ attach。
pub(crate) async fn attach(cfg: &RemoteConfig) -> Result<Replayed, AttachErr> {
    let origin = cfg.origin_label();
    let mut replaced = false;
    ensure(cfg, false).await?;
    loop {
        let (mut r, hello) =
            retry_relay(|| relay_once(cfg), &origin, RELAY_TRIES, RELAY_WAIT).await?;
        let theirs = hello_build(&hello)?;
        // 换不换问本机常驻后端（判定只在后端），这里只照做；手上没带后端字节 ⇒ 不问、照接。
        let verdict = verdict_for(crate::byte_table::my_backend_id(), |mine| {
            ask_verdict(mine, &theirs, replaced)
        })
        .await?;
        if verdict.replace {
            tracing::info!(
                "remote_resident [{origin}] 本机后端判那台常驻后端比手上这一版旧 ⇒ 换掉再接"
            );
            drop(r);
            replaced = true;
            ensure(cfg, true).await?;
            continue;
        }
        let not_sent = |e: std::io::Error| {
            Said::with_raw(
                copy_text(
                    "rsRemoteResident.handshake.attachNotSent",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            )
        };
        r.get_mut()
            .write_all(
                crate::local_backend_host::attach_line(
                    host_core::viewer_tz().as_deref(),
                    Some(&remote_stream_view()),
                )
                .as_bytes(),
            )
            .await
            .map_err(not_sent)?;
        r.get_mut().flush().await.map_err(not_sent)?;
        let reply = read_line(&mut r, false).await?;
        let v: serde_json::Value = serde_json::from_str(&reply).unwrap_or_default();
        if v["attach"] != "ok" {
            let why = match v["reason"].as_str() {
                Some(r) => r.to_string(),
                None => copy_text("rsRemoteResident.handshake.notAttachReply", &[]),
            };
            return Err(copy_text("rsRemoteResident.handshake.refused", &[("why", &why)]).into());
        }
        // 预读进缓冲的字节（attach 之后紧跟的帧）连同 hello 一起补吐。
        let mut head = hello.into_bytes();
        head.push(b'\n');
        head.extend_from_slice(r.buffer());
        r.consume(r.buffer().len());
        tracing::info!("remote_resident [{origin}] 接上常驻后端");
        return Ok(Replayed {
            head: std::io::Cursor::new(head),
            // 接成了就是订阅：摘掉一次性总时限（同流模式那一条）。
            inner: r.into_inner().lives_long(),
            older: verdict.older,
        });
    }
}

/// 「停」的结局 —— 后端 `--resident-stop` 那一行原样的三个词（`control/resident.rs::Stopped`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopWord {
    /// 收到「请你收尾」之后在宽限期内自己退了。
    Graceful,
    /// 宽限期满还在 ⇒ 强杀了（在跑的写可能只做了一半）。
    Killed,
    /// 本来就没在跑。
    NotRunning,
}

/// `backend_stop` 交给机器页的结局：`{stopped, pid}`（本机远端同形）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StopAnswer {
    pub stopped: StopWord,
    pub pid: Option<u32>,
}

/// 读 `--resident-stop` 那一趟（纯函数）：三个词之外的一律是错，不猜（本机那一趟也经它，`local_backend_host::run_resident_stop`）。
pub(crate) fn read_stop(
    exec: &crate::stream_source::RemoteExec,
    machine: &str,
) -> Result<StopAnswer, Said> {
    let v = parse_answer(exec, machine).map_err(AttachErr::said)?;
    let stopped = match v["stopped"].as_str() {
        Some("graceful") => StopWord::Graceful,
        Some("killed") => StopWord::Killed,
        Some("not_running") => StopWord::NotRunning,
        _ => {
            return Err(Said::with_raw(
                copy_text("rsRemoteResident.stop.unknownAnswer", &[]),
                exec.stdout.trim(),
            ))
        }
    };
    let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    Ok(StopAnswer { stopped, pid })
}

/// **停那台的常驻后端**（机器页「停」）：发**一次** `--resident-stop`，等与强杀由那台自己做（同机监督者），这里只拿回结局。
pub(crate) async fn stop(cfg: &RemoteConfig) -> Result<StopAnswer, Said> {
    // 落点固定、打头的 `--` 让那台的 `ccm` 当后端用。
    let cmd = format!(
        "{} {} --resident-stop",
        crate::stream_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    let exec =
        crate::stream_source::connect_and_exec_capture(cfg, &cmd, Some(OLD_BACKEND_MARKER)).await?;
    read_stop(&exec, &cfg.origin_label())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/remote_resident_tests.rs"]
mod tests;
