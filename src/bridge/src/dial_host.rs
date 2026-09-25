//! 〔C2 · `设计/05 §13.2`〕**拨号的宿主**：把一台远端的配置翻成一份拨号请求 · 交给本机后端开一条链路 ·
//! 把链路交给通信层成员 [`crate::ssh_link`] 去读应答。
//!
//! # 〔SR1a · 2026-09-24〕它不再起任何进程
//!
//! 用户裁「改成单一常驻后端」：本机只常驻一个后端，**所有 SSH 连接由它持有、按拨号身份复用**。
//! 这里拿一条到远端的字节流 = 在 monitor 与本机后端之间**那条已有的流**上开一条链路
//! （[`crate::link_mux`]，后端那一半在 `src/backend/dial/link.rs`）。
//! 〔墓碑 —— C2 那一版的原话要点：「定位本机后端二进制 · 起 `<本机后端> --dial` 子进程 ·
//!  把它的两根管子交给 `ssh_link`」「一条链路一个代理子进程」。那一套（二进制解析 · 两个环境变量 ·
//!  起进程）在 SR1a 整段删了。〕
//!
//! # 它为什么不是通信层成员
//!
//! 它做的正是 `05 §2` 的 `C4` 不许成员做的事：读配置、读环境变量（`SSH_AUTH_SOCK`）。
//! 与 `chan/host.rs`（绑回环造钥匙）、`local_backend_host.rs`（起本机后端）同一类 —— **宿主**。
//! 住顶层而不住 `backend/control/`：`backend/` 那一半不许认平台（`the_backend_half_stays_platform_agnostic`），
//! 而 agent 套接字那一格是 Unix 才有的事。
//!
//! # 🔴 没有退路（`D11`：「后端是给定的，不要退路」）
//!
//! 本机后端那条流不在 ⇒ 等一个有界的一会儿（它刚起、hello 还没到的那个窗口），还不在就**报** ——
//! 不起代理进程、不进程内拨 SSH。本机后端不认 `link-open`（比界面老）⇒ **报「太旧」**。
//! 界面进程里的 `russh` 拨号除 SFTP 那一份（SR1b 的事，登记在 `inproc_dial.rs`）外全删了 ——
//! 这里就是界面拿到一条 SSH 链路的**唯一**入口。

use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;

use crate::backend::control::inbound_client::{self, InboundClient};
use crate::link_mux::LinkStream;
use crate::ssh_link::{self, Ack, ConnectStage, LinkError};
use crate::ssh_source::{RemoteConfig, RemoteExec};

/// 等代理回 ack 的上限：握手看门狗（黑洞地址 TCP 连上后握手可以无限阻塞）。与界面侧原来那条
/// `HANDSHAKE_DEADLINE` 同值。后端不许有 `timeout(`（`no_timer_guard`），⇒ 看门狗在这里执行：
/// 到点丢掉链路 = `link-close` = 后端收掉那条链路的拨号任务（连同它手里那些 socket）。
const ACK_DEADLINE: Duration = Duration::from_secs(45);

/// 链路上每条入方向命令（`link-open` / `link-data` / `link-credit` / `link-close`）等应答的上限。
/// `link-data` 的应答在那一块**写进 SSH channel 之后**才回 ⇒ 远端吃得慢时它会等；60 s 与
/// `frame_query` 一页的期限同值。到点 ⇒ 那一次写报错，调用方按连接断了处置。
const LINK_CALL_BUDGET: Duration = Duration::from_secs(60);

/// 本机后端那条流还没登记时，等它多久：`LOCAL_WAIT_TRIES × LOCAL_WAIT_INTERVAL_MS` ≈ 3 s。
/// **`wait-for-condition`**（登记在 `rust_timer_registry`）：等的是一次性条件（本机那条流的 hello 到了），
/// 等到就走、等不到就如实报。对端就在本机，从起进程到 hello 是毫秒级。
const LOCAL_WAIT_TRIES: u32 = 60;
const LOCAL_WAIT_INTERVAL_MS: u64 = 50;

/// ack 之前每一行（阶段 / ack）的字节上限。ack 正常 < 300 字节，64 KiB 是两个数量级以上的余量；
/// 对端坏掉、或压根不是我们的后端时，一条没有换行的巨流不许变成无界堆分配。
fn ack_line_cap() -> u64 {
    64 * 1024
}

/// 本机后端那条流上的入方向客户端（有界地等它出现）。**找不到就报，不回落**（`D11`）。
async fn local_channel() -> Result<Arc<InboundClient>, String> {
    local_backend_accepting("link-open").await
}

/// 〔SR1b〕同上，但问的是**哪一条命令**：本机后端那条流在、且认 `cmd` ⇒ 回它的客户端。
/// 不在 ⇒ 报「本机后端不在」；不认 ⇒ 报「本机后端太旧」。传输台的中继（`sftp_pool.rs`）也从这里拿。
pub(crate) async fn local_backend_accepting(cmd: &str) -> Result<Arc<InboundClient>, String> {
    let local = inbound_client::LOCAL_ORIGIN;
    for attempt in 0..LOCAL_WAIT_TRIES {
        if let Some(c) = inbound_client::client_for(local) {
            if !c.accepts(cmd) {
                return Err(format!(
                    "本机后端太旧：它不认 `{cmd}`（远端的 SSH 与 SFTP 从这一版起由本机后端来做）—— \
                     停掉旧的本机后端、重开 monitor"
                ));
            }
            return Ok(c);
        }
        if attempt + 1 < LOCAL_WAIT_TRIES {
            tokio::time::sleep(Duration::from_millis(LOCAL_WAIT_INTERVAL_MS)).await;
        }
    }
    Err(format!(
        "本机后端不在，远端连不了（远端的 SSH 由本机后端来拨）：等了 {}ms 还没有本机后端那条流。\
         到设置 → 后端里看本机后端的状态",
        u64::from(LOCAL_WAIT_TRIES) * LOCAL_WAIT_INTERVAL_MS
    ))
}

/// 界面进程此刻的 ssh-agent 套接字（Unix）。常驻后端活得比界面长，它自己身上那份可能早就不指向活的 agent
/// ⇒ 由界面交过去（后端 `DialRequest::agent_sock`）。Windows 上 agent 是固定的命名管道，不给。
fn agent_sock() -> Option<String> {
    if cfg!(unix) {
        std::env::var("SSH_AUTH_SOCK")
            .ok()
            .filter(|s| !s.trim().is_empty())
    } else {
        None
    }
}

/// 请求里的一个地址 / 跳板那一台 —— 蛇形键，与后端 `dial::{Endpoint, JumpHop}` 对齐。
fn hop_json(cfg: &RemoteConfig) -> serde_json::Value {
    serde_json::json!({
        "host": cfg.host,
        "port": cfg.port,
        "user": cfg.user,
        "key_path": cfg.key_path,
        "host_key_fingerprint": cfg.host_key_fingerprint,
        "label": cfg.origin_label(),
    })
}

/// 把一台远端的配置翻成一份拨号请求（**只放路径，不放私钥本体** —— 凭据面 `K11`）。
///
/// 竞速顺序由这里按 last-good 排好（记忆住界面进程：代理是短命的，记不住）；
/// 跳板那一台的配置由这里查（`C4`：读配置是宿主的事），环 / 查无当场报错（fail-closed）。
pub(crate) fn request(
    cfg: &RemoteConfig,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let origin = cfg.origin_label();
    let order = crate::ssh_source::winner_order(
        cfg.endpoints(),
        crate::ssh_source::last_good_for(&origin).as_ref(),
    );
    let endpoints: Vec<serde_json::Value> = order
        .iter()
        .map(|e| serde_json::json!({ "host": e.host, "port": e.port }))
        .collect();
    let mut req = serde_json::json!({
        "host": cfg.host,
        "port": cfg.port,
        "user": cfg.user,
        "key_path": cfg.key_path,
        "host_key_fingerprint": cfg.host_key_fingerprint,
        "endpoints": endpoints,
        "use": use_,
        "agent_sock": agent_sock(),
    });
    if let Some(jump_label) = cfg.jump.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if jump_label == origin {
            return Err("跳板配置指向自己（环）".to_string());
        }
        let jump_cfg = crate::load_remote_config_by_label(jump_label)
            .ok_or_else(|| format!("跳板配置未找到: {jump_label}"))?;
        // v1 单跳：跳板自身的 jump 忽略（防链式递归 / 环）。
        req["jump"] = hop_json(&jump_cfg);
    }
    if let (Some(obj), serde_json::Value::Object(more)) = (req.as_object_mut(), extra) {
        obj.extend(more);
    }
    Ok(req)
}

/// 〔SR1b〕传输台那一趟的拨号请求（本机后端开单时读进去、起跑时拿它开 sftp 会话）。
/// 用法写 `files`（SFTP 那一族）；后端的传输台只读身份与鉴权那几项，用法不看。
pub(crate) fn transfer_dial(cfg: &RemoteConfig) -> Result<serde_json::Value, String> {
    request(cfg, "files", serde_json::json!({}))
}

/// 一条**开在本机后端里**的链路（〔SR1a〕C2 那一版是一个 `--dial` 子进程的两根管子）。
///
/// 丢掉这个结构 = 关链路（`link-close`）= 后端收掉它的拨号 / 服务任务；那条 SSH 连接**不跟着断**
/// （同一台远端的别的链路可能还在用它）。
pub struct DialStream {
    /// ⚠ **必须是 `BufReader` 本体**：ack 那一行是按行读的，缓冲里很可能已经预读了后面的字节。
    r: BufReader<LinkStream>,
}

impl tokio::io::AsyncRead for DialStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut self.r), cx, buf)
    }
}

impl tokio::io::AsyncWrite for DialStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(self.r.get_mut()), cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(self.r.get_mut()), cx)
    }
    /// 关写半边 = 关链路（C2 那一版：丢掉子进程的 stdin ⇒ 代理收工）。
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        tokio::io::AsyncWrite::poll_shutdown(std::pin::Pin::new(self.r.get_mut()), cx)
    }
}

/// 开链路、交请求、在 [`ACK_DEADLINE`] 内读完握手。成功 ⇒ 链路 ＋ ack。
///
/// 失败回 `(说法, 看到过的指纹)` —— 测试连接要把指纹给用户看（TOFU 固化 / 失配时比对）。
async fn open(
    cfg: &RemoteConfig,
    req: &serde_json::Value,
    want: &str,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<(DialStream, Ack), (String, Option<String>)> {
    let client = local_channel().await.map_err(|e| (e, None))?;
    // ★ F05 下半的那条埋点跟着拨号搬到这里：量的是「开链路 ＋（池里没有时）TCP ＋ 握手 ＋ 指纹校验 ＋ 鉴权 ＋ 开通道」。
    //   〔SR1a〕同一台远端已经有连接时，这个数只剩「开一条 channel」—— 复用的收益就在这一行里看得见。
    let t_handshake = std::time::Instant::now();
    let link = LinkStream::open(client, req.clone(), LINK_CALL_BUDGET)
        .await
        .map_err(|e| (e, None))?;
    let mut r = BufReader::new(link);
    let shake = ssh_link::handshake(&mut r, want, ack_line_cap(), on_stage);
    let ack = match tokio::time::timeout(ACK_DEADLINE, shake).await {
        Ok(Ok(ack)) => ack,
        Ok(Err(LinkError::Refused { why, fingerprint })) => {
            return Err((why, fingerprint));
        }
        Ok(Err(e)) => return Err((e.to_string(), None)),
        // 到点：`r`（链路）随本函数返回被丢掉 ⇒ `link-close` ⇒ 后端收掉这条链路的拨号。
        Err(_) => {
            return Err((
                format!(
                    "握手超时（{}s 没回应答；地址 {}）",
                    ACK_DEADLINE.as_secs(),
                    cfg.endpoints()
                        .iter()
                        .map(|e| format!("{}:{}", e.host, e.port))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None,
            ))
        }
    };
    // 竞速胜者记成 last-good（下次排首）。
    if let Some(won) = ack
        .endpoint
        .as_deref()
        .and_then(|e| crate::ssh_source::parse_address_line(e, cfg.port))
    {
        crate::ssh_source::record_last_good(&cfg.origin_label(), &won);
    }
    let origin = cfg.origin_label();
    tracing::info!(
        "[perf] ssh_source [{origin}] SSH 握手+鉴权 {}ms（经本机常驻后端的链路：开链路＋[池里没有时]TCP＋握手＋指纹校验＋auth＋开通道；\
         winner={:?}；指纹 {:?}）",
        t_handshake.elapsed().as_millis(),
        ack.endpoint,
        ack.fingerprint
    );
    Ok((DialStream { r }, ack))
}

/// **一条 exec 的字节流**：远端跑 `cmd`，读端是它的 stdout、写端是它的 stdin。
/// 后端长连接流与十来处一次性查询都从这里拿链路。
pub(crate) async fn open_stream(cfg: &RemoteConfig, cmd: &str) -> Result<DialStream, String> {
    let req = request(cfg, "stream", serde_json::json!({ "command": cmd }))?;
    open(cfg, &req, "stream", &mut |_| {})
        .await
        .map(|(s, _)| s)
        .map_err(|(e, _)| e)
}

/// **收全一条 exec**：stdout / stderr / 退出码。`abort_marker` 一出现就提前收（老后端掉进流模式永不 EOF）。
pub(crate) async fn capture(
    cfg: &RemoteConfig,
    cmd: &str,
    abort_marker: Option<&str>,
    max_bytes: usize,
) -> Result<RemoteExec, String> {
    let req = request(
        cfg,
        "capture",
        serde_json::json!({
            "command": cmd,
            "capture": { "max_bytes": max_bytes, "abort_marker": abort_marker },
        }),
    )?;
    let (mut link, _) = open(cfg, &req, "capture", &mut |_| {})
        .await
        .map_err(|(e, _)| e)?;
    // 结果那一行最多是两份 `max_bytes` 加 JSON 转义的开销 —— 上限给四倍。
    let cap = (max_bytes as u64).saturating_mul(4).max(ack_line_cap());
    let got = ssh_link::captured(&mut link.r, cap)
        .await
        .map_err(|e| e.to_string())?;
    Ok(RemoteExec {
        stdout: got.stdout,
        stderr: got.stderr,
        exit_status: got.exit_status,
    })
}

/// **测试连接那一趟**：短命探活，阶段行逐条交给 `on_stage`，exec `cmd` 之后把链路交回（探后端 hello 用）。
/// 失败回 `(说法, 看到过的指纹)`。
pub(crate) async fn probe(
    cfg: &RemoteConfig,
    cmd: &str,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<(DialStream, Ack), (String, Option<String>)> {
    let req = request(
        cfg,
        "stream",
        serde_json::json!({ "command": cmd, "stages": true, "probe": true }),
    )
    .map_err(|e| (e, None))?;
    open(cfg, &req, "stream", on_stage).await
}

/// 一条**端口转发**：本机后端那一侧绑好了本机回环口（`127.0.0.1:local_port`），每接进一条连接开一条隧道。
/// 丢掉它 = 关链路 = 本地口释放、隧道全断（那条 SSH 连接不跟着断，别的链路可能还在用）。
pub struct ForwardLink {
    link: DialStream,
}

impl ForwardLink {
    /// 等下一条「接进了第 n 条连接」。`None` = 链路收尾了（远端断了 / 后端那侧收工了）。
    pub(crate) async fn next_accepted(&mut self) -> Result<Option<u64>, String> {
        ssh_link::accepted(&mut self.link.r, ack_line_cap())
            .await
            .map_err(|e| e.to_string())
    }
}

/// 起一条端口转发（F58）。收的是**机器标签**：查那台的配置是宿主的事（`C4`），
/// 调用方（`port_forward.rs`，通信层成员）手里只有一个地址。
pub(crate) async fn forward(
    origin: &crate::origin::Origin,
    local_port: u16,
    remote_host: &str,
    remote_port: u16,
) -> Result<ForwardLink, String> {
    let origin = origin.as_wire_str();
    let cfg = crate::load_remote_config_by_label(origin)
        .ok_or_else(|| format!("未找到远端配置: {origin}"))?;
    let cfg = &cfg;
    let req = request(
        cfg,
        "forward",
        serde_json::json!({
            "forward": {
                "local_port": local_port,
                "remote_host": remote_host,
                "remote_port": remote_port,
            }
        }),
    )?;
    let (link, _) = open(cfg, &req, "forward", &mut |_| {})
        .await
        .map_err(|(e, _)| format!("连接 {origin} 失败: {e}"))?;
    Ok(ForwardLink { link })
}

#[cfg(test)]
#[path = "../../../tests/bridge/dial_host_tests.rs"]
mod tests;
