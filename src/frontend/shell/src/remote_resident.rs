//! **远端常驻后端，monitor 这一侧**（「远端常驻、本机远端同形」·）。
//!
//! 与本机宿主（`local_backend_host`）同形的四件：起 · 找 · 只升不降 · 停。设计住仓外。
//! - 起 · 找：经链路 `capture` 在远端跑 `--resident-ensure`（口上已有常驻后端 ⇒ 回 `{port, token}`；没有 ⇒ 起一个脱离的自己、
//!   回 `{port, token: null}`，钥匙由它绑上口之后自己写）→ 经链路 `tunnel`（本机常驻后端开 direct-tcpip 到远端回环口）
//!   连上去读 hello；手上没钥匙就再问一次 `--resident-ensure`（此刻口上有人 ⇒ 回盘上那一把）；交 attach 行。
//! - 只升不降：hello 的 build 比手上这一版旧 ⇒ `--resident-ensure --replace` 一次；比我新 ⇒ 照接。
//! 「换不换」由本机常驻后端判（帧命令 `resident-verdict`，与 `deploy-plan` 一家；判定只在后端），这里只照做。
//! - 停：`--resident-stop`（那台自己做「请它收尾 → 宽限期内等 → 到点强杀」，这里只发一次、拿回 `graceful | killed | not_running`）。
//!
//! 远端只有常驻这一形：那台答「脱离不了」（非 unix）⇒ 明说不支持；太旧不认这条子命令 ⇒ 出声报错。
//! 不回落到随 SSH 生死的流模式。
//! ⚠ 钥匙只在内存里过一趟（ensure 的 stdout → attach 行），不进日志、不进报错。

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::copy_table::copy_text;
use crate::dial_host::DialStream;
use crate::ssh_source::RemoteConfig;

/// 起子进程之后等它把口 bind 上：开隧道失败（远端口上还没人）⇒ 隔一会儿再开，封顶这么多次。
const TUNNEL_TRIES: u32 = 30;
const TUNNEL_WAIT: Duration = Duration::from_millis(200);

/// 不认 `--resident-ensure` 的老后端会把它当未知旗标、直接进流模式发 hello ⇒ 见到它就收工、当「太旧」。
const OLD_BACKEND_MARKER: &str = "\"kind\":\"hello\"";

/// `--resident-ensure` 的答。`token` 缺席 = 刚起了一个，钥匙由它绑上口之后自己写（读到 hello 之后再问一次）。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Ensured {
    pub port: u16,
    token: Option<String>,
}

impl std::fmt::Debug for Ensured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ensured {{ port: {}, token: … }}", self.port)
    }
}

/// 为什么没接上那台的常驻后端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachErr {
    /// 那台不是 Unix（后端脱离不了）⇒ **永久不支持**：记在那台的连接状态里，
    /// 不再自动按退避重连；界面出声，用户点「起」（`backend_start`）才再试一次。
    /// 那台 sshd 不许端口转发（控制隧道被回拒 `administratively_prohibited`）同属这一形：重试不会变，要那台改配置。
    Unsupported(String),
    /// 别的失败 ⇒ 照常按退避重连。
    Failed(String),
}

impl From<String> for AttachErr {
    fn from(s: String) -> Self {
        AttachErr::Failed(s)
    }
}

impl AttachErr {
    /// 给人看的那句。
    pub(crate) fn said(self) -> String {
        match self {
            AttachErr::Unsupported(s) | AttachErr::Failed(s) => s,
        }
    }
}

/// 读 `--resident-ensure` / `--resident-stop` 那一趟的结果（纯函数）：退出 0 ⇒ stdout 那一行；退出 2 ⇒ stderr 的 `{code,message}`。
/// `unsupported`（那台脱离不了，非 unix）明说「远端只支持 Unix」；老后端 ⇒ 「太旧」—— 都是失败，没有回落。
pub(crate) fn parse_answer(
    exec: &crate::ssh_source::RemoteExec,
) -> Result<serde_json::Value, AttachErr> {
    if exec.stdout.contains(OLD_BACKEND_MARKER) {
        return Err(copy_text("rsRemoteResident.ensure.tooOld", &[]).into());
    }
    match exec.exit_status {
        Some(0) => serde_json::from_str(exec.stdout.trim()).map_err(|e| {
            copy_text(
                "rsRemoteResident.ensure.answerUnreadable",
                &[("e", &e.to_string())],
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
            if err["code"] == "unsupported" {
                Err(AttachErr::Unsupported(copy_text(
                    "rsRemoteResident.ensure.unsupported",
                    &[("why", &msg)],
                )))
            } else {
                Err(AttachErr::Failed(msg))
            }
        }
    }
}

fn parse_ensured(v: &serde_json::Value) -> Result<Ensured, String> {
    let port = v["port"]
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p != 0);
    let token = v["token"].as_str().filter(|t| !t.is_empty());
    match port {
        Some(port) => Ok(Ensured {
            port,
            token: token.map(str::to_string),
        }),
        None => Err(copy_text("rsRemoteResident.ensure.answerIncomplete", &[])),
    }
}

async fn ensure(cfg: &RemoteConfig, replace: bool) -> Result<Ensured, AttachErr> {
    // `ccm -- --resident-ensure`（打头的 `--` 让那台的 `ccm` 当后端用）。
    let mut cmd = format!(
        "{} {} --resident-ensure",
        crate::ssh_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    if replace {
        cmd.push_str(" --replace");
    }
    let exec =
        crate::ssh_source::connect_and_exec_capture(cfg, &cmd, Some(OLD_BACKEND_MARKER)).await?;
    Ok(parse_ensured(&parse_answer(&exec)?)?)
}

/// hello 那一行里那台报的 build（纯函数，只读线上形状）。口上不是常驻后端（第一行不是 hello）⇒ `Err`（那句话）。
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
pub(crate) fn decode_verdict(v: &serde_json::Value) -> Result<Verdict, String> {
    let bad = |what: &str| {
        copy_text(
            "rsRemoteResident.verdict.unreadable",
            &[("e", &what.to_string())],
        )
    };
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

/// 问本机常驻后端「那台报 `theirs`，换还是接」。入参只有事实（手上这一版 · 那台报的 · 这一趟换过没有）。
async fn ask_verdict(mine: &str, theirs: &str, replaced: bool) -> Result<Verdict, String> {
    use crate::backend_route::{route_call_error, Routed};
    let client = crate::dial_host::local_backend_accepting(VERDICT_CMD).await?;
    let args = serde_json::json!({ "mine": mine, "theirs": theirs, "replaced": replaced });
    let data = client
        .call(VERDICT_CMD, args, VERDICT_BUDGET)
        .await
        .map_err(
            |e| match route_call_error(&e, |_code, message| message.to_string()) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
            },
        )?;
    decode_verdict(&data.unwrap_or_default())
}

/// attach 行：钥匙 ＋ 这条连接要的流模式旗标（远端 `listen::attach_flags` 的逆）。
pub(crate) fn attach_line(token: &str, flags: (bool, bool)) -> String {
    let (with_bg, tail_only) = flags;
    let mut f: Vec<&str> = Vec::new();
    if with_bg {
        f.push("--with-bg");
    }
    if tail_only {
        f.push("--tail-only");
    }
    let mut line = serde_json::json!({ "attach": token, "flags": f }).to_string();
    line.push('\n');
    line
}

/// 读握手那一行（hello / attach 应答）；`hello` 选哪一句说「没答完」。
async fn read_line(
    r: &mut tokio::io::BufReader<DialStream>,
    hello: bool,
) -> Result<String, String> {
    let mut buf = Vec::new();
    let got = crate::ssh_source::read_capped_line(
        r,
        &mut buf,
        // hello / attach 应答那一行：与本机宿主同一个上限（同一条监听协议）。
        crate::local_backend_host::LISTEN_HANDSHAKE_LINE_CAP,
    )
    .await;
    match got {
        Ok(crate::ssh_source::CappedLine::Line) => Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches(['\n', '\r'])
            .to_string()),
        Ok(_) if hello => Err(copy_text("rsRemoteResident.handshake.helloCut", &[])),
        Ok(_) => Err(copy_text("rsRemoteResident.handshake.replyCut", &[])),
        Err(e) => Err(copy_text(
            "rsRemoteResident.handshake.readFailed",
            &[("e", &e.to_string())],
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

/// 开隧道：远端口上还没人（子进程刚起、还没 bind）⇒ 隔 [`TUNNEL_WAIT`] 再开，至多 [`TUNNEL_TRIES`] 次。
async fn tunnel_when_bound(cfg: &RemoteConfig, port: u16) -> Result<DialStream, AttachErr> {
    retry_tunnel(|| crate::dial_host::tunnel(cfg, port), port).await
}

/// 远端回拒开通道、原因码是这个 ⇒ 那台 sshd 不许端口转发（`AllowTcpForwarding no` · `DisableForwarding` ·
/// authorized_keys 的 `no-port-forwarding` / `permitopen` 都回它；口上没人听回的是 `connect_failed`）。
pub(crate) const FORWARDING_PROHIBITED: &str = "administratively_prohibited";

/// [`tunnel_when_bound`] 的编排（`open` = 开一次隧道；判据用替身）。回拒码是
/// [`FORWARDING_PROHIBITED`] ⇒ **当场停**、[`AttachErr::Unsupported`]（重试不会变：从前这里照样再开 29 次，每次一条新 SSH，
/// 接着整条流按退避重连 —— 真机每分钟 33 条拨号）；别的失败（口上还没人）照旧隔一会儿再开。
pub(crate) async fn retry_tunnel<T, F, Fut>(mut open: F, port: u16) -> Result<T, AttachErr>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, (String, Option<String>)>>,
{
    let mut last = String::new();
    for _ in 0..TUNNEL_TRIES {
        match open().await {
            Ok(s) => return Ok(s),
            Err((_, Some(code))) if code == FORWARDING_PROHIBITED => {
                return Err(AttachErr::Unsupported(copy_text(
                    "rsRemoteResident.tunnel.forwardingProhibited",
                    &[],
                )));
            }
            Err((e, _)) => last = e,
        }
        tokio::time::sleep(TUNNEL_WAIT).await;
    }
    Err(AttachErr::Failed(copy_text(
        "rsRemoteResident.tunnel.unreachable",
        &[("port", &port.to_string()), ("e", &last)],
    )))
}

/// **接上那台的常驻后端**（没有就起一个）：起 · 找 → 隧道 → hello（旧 ⇒ 换一次）→ attach。
pub(crate) async fn attach(cfg: &RemoteConfig, flags: (bool, bool)) -> Result<Replayed, AttachErr> {
    let origin = cfg.origin_label();
    let mut replaced = false;
    let mut ensured = ensure(cfg, false).await?;
    loop {
        let link = tunnel_when_bound(cfg, ensured.port).await?;
        let mut r = tokio::io::BufReader::new(link);
        let hello = read_line(&mut r, true).await?;
        let theirs = hello_build(&hello)?;
        // 换不换问本机常驻后端（判定只在后端），这里只照做。
        let verdict = ask_verdict(
            crate::ssh_source::EXPECTED_BACKEND_BUILD_ID,
            &theirs,
            replaced,
        )
        .await?;
        if verdict.replace {
            tracing::info!(
                "remote_resident [{origin}] 本机后端判那台常驻后端比手上这一版旧 ⇒ 换掉再接"
            );
            drop(r);
            replaced = true;
            ensured = ensure(cfg, true).await?;
            continue;
        }
        // 刚起的那一个：钥匙是它绑上口之后自己写的（每次起都换一把）⇒ 读到 hello 之后再问一次，读盘上那一份。
        let token = match ensured.token.take() {
            Some(t) => t,
            None => ensure(cfg, false)
                .await?
                .token
                .ok_or_else(|| copy_text("rsRemoteResident.ensure.answerIncomplete", &[]))?,
        };
        let not_sent = |e: std::io::Error| {
            copy_text(
                "rsRemoteResident.handshake.attachNotSent",
                &[("e", &e.to_string())],
            )
        };
        r.get_mut()
            .write_all(attach_line(&token, flags).as_bytes())
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
        tracing::info!(
            "remote_resident [{origin}] 接上常驻后端（口 {}）",
            ensured.port
        );
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
pub(crate) fn read_stop(exec: &crate::ssh_source::RemoteExec) -> Result<StopAnswer, String> {
    let v = parse_answer(exec).map_err(AttachErr::said)?;
    let stopped = match v["stopped"].as_str() {
        Some("graceful") => StopWord::Graceful,
        Some("killed") => StopWord::Killed,
        Some("not_running") => StopWord::NotRunning,
        _ => {
            return Err(copy_text(
                "rsRemoteResident.stop.unknownAnswer",
                &[("line", &exec.stdout.trim().to_string())],
            ))
        }
    };
    let pid = v["pid"].as_u64().and_then(|p| u32::try_from(p).ok());
    Ok(StopAnswer { stopped, pid })
}

/// **停那台的常驻后端**（机器页「停」）：发**一次** `--resident-stop`，等与强杀由那台自己做（同机监督者），这里只拿回结局。
pub(crate) async fn stop(cfg: &RemoteConfig) -> Result<StopAnswer, String> {
    // 落点固定、打头的 `--` 让那台的 `ccm` 当后端用。
    let cmd = format!(
        "{} {} --resident-stop",
        crate::ssh_source::BACKEND_CMD,
        crate::local_backend::BACKEND_SEP
    );
    let exec =
        crate::ssh_source::connect_and_exec_capture(cfg, &cmd, Some(OLD_BACKEND_MARKER)).await?;
    read_stop(&exec)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/remote_resident_tests.rs"]
mod tests;
