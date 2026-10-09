//! **拨号的宿主**：把一台远端的配置翻成一份拨号请求 · 交给本机后端开一条链路 ·
//! 把链路交给通信层成员 [`crate::ssh_link`] 去读应答。
//!
//! # 它不再起任何进程
//!
//! 本机只常驻一个后端，**所有 SSH 连接由它持有、按拨号身份复用**。
//! 这里拿一条到远端的字节流 = 在 monitor 与本机后端之间**那条已有的流**上开一条链路
//! （[`crate::link_mux`]，后端那一半在 `src/backend/dial/link.rs`）。
//! 〔墓碑 —— C2 那一版的原话要点：「定位本机后端二进制 · 起 `<本机后端> --dial` 子进程 ·
//!  把它的两根管子交给 `ssh_link`」「一条链路一个代理子进程」。那一套（二进制解析 · 两个环境变量 ·
//!  起进程）在 SR1a 整段删了。〕
//!
//! # 它为什么不是通信层成员
//!
//! 它做的正是 `C4` 不许成员做的事：读配置、读环境变量（`SSH_AUTH_SOCK`）。
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

use crate::copy_table::copy_text;
use crate::detail::Said;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;

use crate::inbound_client::{self, InboundClient};
use crate::link_mux::LinkStream;
use crate::ssh_link::{self, Ack, ConnectStage, LinkError};
use crate::stream_source::{RemoteConfig, RemoteExec};

/// 等代理回 ack 的上限：握手看门狗（黑洞地址 TCP 连上后握手可以无限阻塞）。与界面侧原来那条
/// `HANDSHAKE_DEADLINE` 同值。后端不许有 `timeout(`（`no_timer_guard`），⇒ 看门狗在这里执行：
/// 到点丢掉链路 = `link-close` = 后端收掉那条链路的拨号任务（连同它手里那些 socket）。
const ACK_DEADLINE: Duration = Duration::from_secs(45);

/// **一次性那一趟的总时限**：开链路 → 握手（其中握手另受 [`ACK_DEADLINE`]）→ 远端跑 → 读完，
/// 从开链路那一刻起算**一个绝对时刻**（一次调用一个绝对时刻，不是每跳一个 `Duration`）。
///
/// 为什么非有不可（红线「先装期限再复用」）：连接复用之后，一条卡住的一次性查询占着
/// 池里那条共享连接的一格、永不释放 —— 局部卡死升级成全局卡死。后端零定时器（`no_timer_guard`）⇒ 期限只能在
/// 调用方这一侧执行；到点 ⇒ 读写报 `TimedOut` ⇒ 调用方返回、丢掉链路 ⇒ `link-close` ⇒ 后端收掉那个任务、格还回去。
///
/// **默认有，豁免要点名**：[`open`] 开出来的每一条链路出生就带着它；本来就该长活的三形
/// （后端长连接流 · 端口转发 · 部署文件面 —— 后者每一问自带期限）显式调 [`DialStream::lives_long`] 摘掉，
/// 那三处由 `dial_host_tests::only_the_three_long_lived_links_drop_the_deadline` 两向钉住。
///
/// 值：今天各调用方外面套的最宽是「握手 45 s ＋ 读 30 s」、当时账号工具的部署整趟 45 s ⇒ 120 s 不收紧任何一条既有的；
/// 它是**天花板**（调用方外面再套的更短期限照旧先到）。⚠ 「值归后端」这一格没做到（与 [`ACK_DEADLINE`] 同住这里）。
pub(crate) const ONE_SHOT_DEADLINE: Duration = Duration::from_secs(120);

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

/// 同上，但问的是**哪一条命令**：本机后端那条流在、且认 `cmd` ⇒ 回它的客户端。
/// 不在 ⇒ 报「本机后端不在」；不认 ⇒ 报「本机后端太旧」。传输台的中继（`sftp_pool.rs`）也从这里拿。
pub(crate) async fn local_backend_accepting(cmd: &str) -> Result<Arc<InboundClient>, String> {
    let local = inbound_client::LOCAL_ORIGIN;
    for attempt in 0..LOCAL_WAIT_TRIES {
        if let Some(c) = inbound_client::client_for(local) {
            if !c.accepts(cmd) {
                return Err(copy_core::backend_old(&copy_core::local_machine()));
            }
            return Ok(c);
        }
        if attempt + 1 < LOCAL_WAIT_TRIES {
            tokio::time::sleep(Duration::from_millis(LOCAL_WAIT_INTERVAL_MS)).await;
        }
    }
    Err(copy_text(
        "rsDialHost.local.absent",
        &[(
            "ms",
            &(u64::from(LOCAL_WAIT_TRIES) * LOCAL_WAIT_INTERVAL_MS).to_string(),
        )],
    ))
}

/// 界面进程此刻的 ssh-agent 套接字（Unix）。常驻后端活得比界面长，它自己身上那份可能早就不指向活的 agent
/// ⇒ 由界面交过去（后端 `DialRequest::agent_sock`）。Windows 上 agent 是固定的命名管道，不给。
fn agent_sock() -> Option<String> {
    if crate::platform::ssh_agent::AGENT_VIA_SOCKET_ENV {
        std::env::var("SSH_AUTH_SOCK")
            .ok()
            .filter(|s| !s.trim().is_empty())
    } else {
        None
    }
}

/// 〔「一个判定一个家」〕把一台远端**原样的配置**交给本机常驻后端，拨号请求由它组
/// （`src/backend/dial/machine.rs::resolve`：地址解析 · 指纹继承 · 跳板查无 / 环都在那里）。**只放路径，不放私钥本体**（`K11`）。
///
/// 这里只交宿主手里的几样事实（`C4`：读配置是宿主的事）：
/// - `machine`：这一台（`RemoteConfig` 的 camelCase 形状，与界面那一格同形）；
/// - `saved`：盘上同名的那一份（`stream_source::run` 手里那份是起来时读的 —— 固化指纹之后，靠它让之后的重连转严格）；
/// - `jump`：跳板那一台的配置（查不到 ⇒ `null`，后端照 fail-closed 拒）；
/// - `prefer`：上次赢的那条（结构化，[`crate::stream_source::last_good_for`]；配置改过就失效）；
/// - `agent_sock`：界面进程此刻的 ssh-agent（常驻后端活得比界面长）。
pub(crate) fn request(
    cfg: &RemoteConfig,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut req = machine_facts(cfg);
    if let Some(obj) = req.as_object_mut() {
        obj.insert("use".into(), serde_json::json!(use_));
        obj.insert("agent_sock".into(), serde_json::json!(agent_sock()));
    }
    if let (Some(obj), serde_json::Value::Object(more)) = (req.as_object_mut(), extra) {
        obj.extend(more);
    }
    Ok(req)
}

/// 一台远端**原样的配置**那几格：`{machine, saved, jump, prefer}`（[`request`] 头注逐格）。
///
/// 拆出来是给「开终端」那一问（本机后端帧命令 `terminal-ssh`）：它要同一份机器事实，但不开链路 ⇒ 不要 `use` / `agent_sock`。
/// 界面经 Tauri 命令 [`crate::launch::terminal_dial`] 拿，文件窗口在进程内直接拿；组请求与渲染都在本机后端（`dial/machine.rs::resolve`）。
pub(crate) fn machine_facts(cfg: &RemoteConfig) -> serde_json::Value {
    let origin = cfg.origin_label();
    let jump = cfg
        .jump
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != origin)
        // 写成显式调用（不写成 point-free）：`local_origin_registry` 那条护栏按调用形状数「查远端配置」的点，这一处要被它看见。
        .and_then(|jump_label| crate::load_remote_config_by_label(jump_label));
    serde_json::json!({
        "machine": cfg,
        "saved": crate::load_remote_config_by_label(&origin),
        "jump": jump,
        "prefer": crate::stream_source::last_good_for(cfg).map(|e| serde_json::json!({ "host": e.host, "port": e.port })),
    })
}

/// 传输台那一趟的拨号请求（本机后端开单时读进去、起跑时拿它开 sftp 会话）。
/// 用法写 `files`（SFTP 那一族）；后端的传输台只读身份与鉴权那几项，用法不看。
pub(crate) fn transfer_dial(cfg: &RemoteConfig) -> Result<serde_json::Value, String> {
    request(cfg, "files", serde_json::json!({}))
}

/// 总时限落在链路读写上的那一层：到点之后每一次读 / 写都报 `TimedOut`，不再碰里面那条链路。
///
/// 它是**一次性的上界**（与 `tokio::time::timeout_at` 同一件事），不是节拍：到点那一刻叫醒一次等着的读写，此后不再醒。
/// 关写半边（`shutdown` = 关链路）不受它管 —— 到点之后调用方照样要能把链路关掉。
pub(crate) struct Bounded<S> {
    inner: S,
    /// 到点那一刻 ＋ 用来说话的几样。`None` = 不设（长活那三形）。
    due: Option<Due>,
}

/// 〔「一个预算、多个归因点」〕期限只有一个，但到点时要说得出**卡在哪一段**：
/// 同一个数盖着「握手」与「远端跑」两段（小病：弱网上握手慢一点就被判查询超时，而两种成因处置完全不同）。
struct Due {
    at: std::pin::Pin<Box<tokio::time::Sleep>>,
    /// 总时限（只用来说话）。
    total: Duration,
    /// 起算那一刻（= 到点 − 总时限）。
    started: tokio::time::Instant,
    /// 握手做完的那一刻（[`Bounded::mark_shaken`]；`open` 读完 ack 就记）。`None` = 还在握手。
    shaken: Option<tokio::time::Instant>,
}

impl<S> Bounded<S> {
    pub(crate) fn new(inner: S, due: Option<(tokio::time::Instant, Duration)>) -> Self {
        Bounded {
            inner,
            due: due.map(|(at, total)| Due {
                at: Box::pin(tokio::time::sleep_until(at)),
                total,
                started: at.checked_sub(total).unwrap_or(at),
                shaken: None,
            }),
        }
    }

    /// 握手（开链路 ＋ 拨号 ＋ 鉴权 ＋ ack）做完了：记下这一刻，到点时据此把总时限拆成两段说。
    pub(crate) fn mark_shaken(&mut self) {
        if let Some(d) = self.due.as_mut() {
            d.shaken.get_or_insert_with(tokio::time::Instant::now);
        }
    }

    /// 到点了 ⇒ 那句话；没到 ⇒ `None`（顺手把「到点叫醒我」登记上）。
    fn expired(&mut self, cx: &mut std::task::Context<'_>) -> Option<std::io::Error> {
        let d = self.due.as_mut()?;
        std::future::Future::poll(d.at.as_mut(), cx)
            .is_ready()
            .then(|| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    expiry_note(
                        d.total,
                        d.shaken.map(|t| t.saturating_duration_since(d.started)),
                        tokio::time::Instant::now().saturating_duration_since(d.started),
                    ),
                )
            })
    }
}

/// 到点那句话：**一个预算、按段归因**。纯函数（判据直接喂时长，不睡墙钟）。
/// `shaken` = 起算之后多久握完手（`None` = 到点时还在握手）；`waited` = 起算到此刻。
pub(crate) fn expiry_note(total: Duration, shaken: Option<Duration>, waited: Duration) -> String {
    let dur = copy_core::format_elapsed(total);
    match shaken {
        None => copy_text("rsDialHost.deadline.expiredInShake", &[("dur", &dur)]),
        Some(shake) => copy_text(
            "rsDialHost.deadline.expired",
            &[
                ("dur", &dur),
                ("shake", &copy_core::format_elapsed(shake)),
                (
                    "run",
                    &copy_core::format_elapsed(waited.saturating_sub(shake)),
                ),
            ],
        ),
    }
}

impl<S: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Bounded<S> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(e) = self.expired(cx) {
            return std::task::Poll::Ready(Err(e));
        }
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Bounded<S> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if let Some(e) = self.expired(cx) {
            return std::task::Poll::Ready(Err(e));
        }
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(e) = self.expired(cx) {
            return std::task::Poll::Ready(Err(e));
        }
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// 一条**开在本机后端里**的链路（C2 那一版是一个 `--dial` 子进程的两根管子）。
///
/// 丢掉这个结构 = 关链路（`link-close`）= 后端收掉它的拨号 / 服务任务；那条 SSH 连接**不跟着断**
/// （同一台远端的别的链路可能还在用它）。
///
/// 出生就带着 [`ONE_SHOT_DEADLINE`]（[`open`] 起算）；长活那三形调 [`DialStream::lives_long`] 摘掉。
pub struct DialStream {
    /// ⚠ **必须是 `BufReader` 本体**：ack 那一行是按行读的，缓冲里很可能已经预读了后面的字节。
    /// 期限那一层垫在 `BufReader` 底下 ⇒ 握手那几行、收全那一行、调用方自己的读写全在同一个时限里。
    r: BufReader<Bounded<LinkStream>>,
}

impl DialStream {
    /// **摘掉总时限**：只给本来就该长活的那三形（后端长连接流 · 端口转发 · 部署文件面）。
    /// 调用点由 `dial_host_tests::only_the_three_long_lived_links_drop_the_deadline` 两向钉住 ——
    /// 多一处 = 又有一条一次性的路没有总时限（红线）。
    pub(crate) fn lives_long(mut self) -> Self {
        self.r.get_mut().due = None;
        self
    }
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
/// 失败回 `(那一句 ＋ 复制详情, 开通道被远端回拒的原因码)` —— 原因码只有 `tunnel` 那一形会有（[`tunnel`] 的调用方据它决定停不停）。
/// 后端说没拨成 ⇒ 详情是它写的那一份（远端补「本机」一行，[`crate::detail::relayed`]）；读应答这一跳坏了 ⇒ 壳写、原话进详情。
async fn open(
    cfg: &RemoteConfig,
    req: &serde_json::Value,
    want: &str,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<(DialStream, Ack), (Said, Option<String>)> {
    // 这一趟的总时限从这一刻起算（开链路本身也在里面）。
    let due = tokio::time::Instant::now() + ONE_SHOT_DEADLINE;
    let client = local_channel().await.map_err(|e| (Said::from(e), None))?;
    // ★ F05 下半的那条埋点跟着拨号搬到这里：量的是「开链路 ＋（池里没有时）TCP ＋ 握手 ＋ 指纹校验 ＋ 鉴权 ＋ 开通道」。
    // 同一台远端已经有连接时，这个数只剩「开一条 channel」—— 复用的收益就在这一行里看得见。
    let t_handshake = std::time::Instant::now();
    let link = LinkStream::open(client, req.clone(), LINK_CALL_BUDGET)
        .await
        .map_err(|e| (Said::from(e), None))?;
    let mut r = BufReader::new(Bounded::new(link, Some((due, ONE_SHOT_DEADLINE))));
    let shake = ssh_link::handshake(&mut r, want, ack_line_cap(), on_stage);
    let ack = match tokio::time::timeout(ACK_DEADLINE, shake).await {
        Ok(Ok(ack)) => ack,
        Ok(Err(LinkError::Refused {
            why,
            open_refused,
            reason,
            fingerprint,
            detail,
        })) => {
            let said = refused_said(why, detail.as_deref());
            // 后端带回的原因码 · 那台出示的指纹 · 那一句与详情进那台的状态成品（这一轮收尾时按码说那一句、给修法，详情跟着）。
            crate::machine_state::dial_failed(
                &cfg.origin_label(),
                reason.as_deref(),
                fingerprint.as_deref(),
                &said,
            );
            return Err((said, open_refused));
        }
        Ok(Err(e)) => return Err((link_said(e), None)),
        // 到点：`r`（链路）随本函数返回被丢掉 ⇒ `link-close` ⇒ 后端收掉这条链路的拨号。
        Err(_) => {
            return Err((
                Said::from(copy_text(
                    "rsDialHost.open.timeout",
                    &[
                        ("dur", &copy_core::format_elapsed(ACK_DEADLINE)),
                        (
                            "addr",
                            // 原样列出配置里那几行（地址不在这里解析，组法在后端 `dial/machine.rs`）。
                            &std::iter::once(format!("{}:{}", cfg.host, cfg.port))
                                .chain(
                                    cfg.addresses
                                        .iter()
                                        .map(|a| a.trim().to_string())
                                        .filter(|a| !a.is_empty()),
                                )
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ],
                )),
                None,
            ));
        }
    };
    // 握手做完了 ⇒ 记下这一刻：之后若总时限到点，那句话说得出「握手用了多久、远端跑了多久」。
    r.get_mut().mark_shaken();
    // ack 成功 = 后端那边鉴权已过 ⇒ 判要不要自动固化。
    settle_host_key(cfg, req, &ack);
    // 竞速胜者记成 last-good（下次当 `prefer` 交回去排首）—— 后端交的是结构化的 `winner`，这里不解析地址。
    if let Some(won) = &ack.winner {
        crate::stream_source::record_last_good(cfg, won);
    }
    let origin = cfg.origin_label();
    tracing::info!(
        "[perf] stream_source [{origin}] SSH 握手+鉴权 {}ms（经本机常驻后端的链路：开链路＋[池里没有时]TCP＋握手＋指纹校验＋auth＋开通道；\
         winner={:?}；指纹 {:?}）",
        t_handshake.elapsed().as_millis(),
        ack.endpoint,
        ack.fingerprint
    );
    Ok((DialStream { r }, ack))
}

/// 本机后端说没拨成 / 没做成：那一句 ＋ 它写好的详情（拨号与 files 链路都跑在**本机**常驻后端里 ⇒ 原样，不补「本机」行）；
/// 老后端没写详情 ⇒ 壳写时刻与本机。
pub(crate) fn refused_said(why: String, detail: Option<&str>) -> Said {
    match detail.filter(|d| !d.trim().is_empty()) {
        Some(d) => Said {
            said: why,
            detail: d.to_string(),
        },
        None => Said::from(why),
    }
}

/// 读应答这一跳坏了（管子读错 · 形状不对）：那一句不带原话，原话进详情。
pub(crate) fn link_said(e: LinkError) -> Said {
    match &e {
        LinkError::Io(raw) | LinkError::Garbled(raw) => Said::with_raw(e.to_string(), raw),
        _ => Said::from(e.to_string()),
    }
}

// ═══ 〔「自动固化 ＋ 默认转严格 ＋ 保住多地址那一格」〕host key 自动固化 ═══════════════

/// 一次成功拨号之后，host key 这一格怎么办。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PinVerdict {
    /// 测试连接（`probe`）：不自动固化，机器页那颗按钮照旧。
    NotAsked,
    /// 配置里已有指纹：这一趟已是严格校验。
    AlreadyStrict,
    /// ack 里没有逐地址指纹（老后端）：判不了「各地址一致」⇒ 不固化。
    NoneReported,
    /// 报过指纹的每条地址都是这一个 ⇒ 固化它。
    Pin(String),
    /// 各地址报的不一样 ⇒ 不固化，说出来让人选。
    Differs(std::collections::BTreeMap<String, String>),
}

/// 🔴 判定只此一处。`strict` = 这一趟是不是已经严格校验（后端在 ack 里说）。
pub(crate) fn pin_verdict(
    probe: bool,
    strict: bool,
    reported: &std::collections::BTreeMap<String, String>,
) -> PinVerdict {
    if probe {
        return PinVerdict::NotAsked;
    }
    if strict {
        return PinVerdict::AlreadyStrict;
    }
    let mut fps = reported.values();
    let Some(first) = fps.next() else {
        return PinVerdict::NoneReported;
    };
    if fps.all(|f| f == first) {
        PinVerdict::Pin(first.clone())
    } else {
        PinVerdict::Differs(reported.clone())
    }
}

/// 那一刻的 UTC 日期 `YYYY-MM-DD`（公历，按天数换算，不引日期库）。
pub(crate) fn utc_day(t: std::time::SystemTime) -> String {
    let days = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = copy_core::civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 固化那一次写的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinWrite {
    Written,
    /// 盘上那一台已有指纹（别人刚设过）⇒ 不写。
    AlreadySet,
    NotFound,
    /// 同一个 origin ＋ host 有不止一台 ⇒ 不知道写哪台，不写。
    Ambiguous,
}

/// 往 `config.json` 的 `remote.hosts` 里那一台写指纹：经补丁口（`config::patch_config_at`）交一条**按键认数组元素**的
/// `SetIn`（锁内现读现判：认 origin（`label` 非空取它、否则 `host`，同 `lib.rs::parse_host_obj`）与 `host`
/// 都相等的恰好一台，只改它的 `hostKeyFingerprint`，已有值不动 —— CAS）。别的机器、别的格与设置页同时写的改动都不会被盖掉。
pub(crate) fn pin_host_key_at(
    path: &std::path::Path,
    origin: &str,
    host: &str,
    fp: &str,
) -> Result<PinWrite, String> {
    use crate::config::{Applied, ConfigEdit, ConfigWriteError, ElemKey};
    let key = |fields: &[&str], equals: &str| ElemKey {
        fields: fields.iter().map(|f| f.to_string()).collect(),
        equals: equals.to_string(),
    };
    let edit = ConfigEdit::SetIn {
        path: vec!["remote".into(), "hosts".into()],
        r#where: vec![key(&["label", "host"], origin), key(&["host"], host)],
        field: "hostKeyFingerprint".into(),
        value: serde_json::Value::String(fp.to_string()),
        if_empty: true,
    };
    match crate::config::patch_config_at(path, &[edit]) {
        Ok(applied) => Ok(match applied.first() {
            Some(Applied::Done) => {
                // 记下是哪一天记的（只给人看：设置页「主机指纹 · 记于 …」）。写不上不影响已经写下的指纹。
                let day = ConfigEdit::SetIn {
                    path: vec!["remote".into(), "hosts".into()],
                    r#where: vec![key(&["label", "host"], origin), key(&["host"], host)],
                    field: "hostKeyPinnedAt".into(),
                    value: serde_json::Value::String(utc_day(std::time::SystemTime::now())),
                    if_empty: false,
                };
                if let Err(e) = crate::config::patch_config_at(path, &[day]) {
                    tracing::warn!("[{origin}] 指纹记下的日子没写上：{e}");
                }
                PinWrite::Written
            }
            Some(Applied::Kept) => PinWrite::AlreadySet,
            _ => PinWrite::NotFound,
        }),
        Err(ConfigWriteError::NoSuchElement { ambiguous: true }) => Ok(PinWrite::Ambiguous),
        Err(ConfigWriteError::NoSuchElement { ambiguous: false }) => Ok(PinWrite::NotFound),
        Err(e) => Err(e.to_string()),
    }
}

// 「这一趟交给后端的指纹」（`cfg` 里有就用，没有 ⇒ 盘上同一台的）那条继承规则搬进后端 `dial/machine.rs::request`
//   （宿主把盘上那一份当 `saved` 交过去）；这一趟是不是严格校验，由后端在 ack 里说（`strict` / `jump_strict`）。

/// 告知界面的那一件（经 `lib.rs` 装的出口发 `remote-health`，kind 见下面两个常量）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyNotice {
    pub origin: String,
    pub kind: &'static str,
    pub message: String,
}
pub const HOST_KEY_PINNED: &str = "host_key_pinned";
pub const HOST_KEY_DIFFERS: &str = "host_key_differs";

type NoticeSink = Box<dyn Fn(HostKeyNotice) + Send + Sync>;
static NOTICE_SINK: std::sync::OnceLock<NoticeSink> = std::sync::OnceLock::new();

/// 装出口（`lib.rs` setup 装一次；同 `session_facts::install_sink`）。
pub fn install_host_key_notice(f: impl Fn(HostKeyNotice) + Send + Sync + 'static) {
    if NOTICE_SINK.set(Box::new(f)).is_err() {
        tracing::warn!("host key 告知的出口装了第二次 —— 忽略");
    }
}

/// 同一句话本进程只说一次（每条一次性查询都开链路，不去重就刷屏）。
fn notice(origin: String, kind: &'static str, message: String) {
    static SAID: std::sync::Mutex<Vec<(String, &str, String)>> = std::sync::Mutex::new(Vec::new());
    let key = (origin.clone(), kind, message.clone());
    {
        let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
        if said.contains(&key) {
            return;
        }
        said.push(key);
    }
    tracing::warn!("[{origin}] {message}");
    if let Some(f) = NOTICE_SINK.get() {
        f(HostKeyNotice {
            origin,
            kind,
            message,
        });
    }
}

/// 成功拨号之后：判 → 固化 / 说出来。目标那一台一格；经跳板时跳板那一台另一格（〔第二问〕它是另一台机器，
/// 按它自己在 `remote.hosts` 里那一条固化 —— 只当跳板用、从不直连的那台也不再一直 TOFU）。两格同一个判定（[`pin_verdict`]）。
pub(crate) fn settle_host_key(cfg: &RemoteConfig, req: &serde_json::Value, ack: &Ack) {
    let probe = req.get("probe").and_then(serde_json::Value::as_bool) == Some(true);
    let Some(path) = crate::config::resolve_config_path() else {
        return;
    };
    for (origin, host, strict, reported) in pin_targets(cfg, req, ack) {
        settle_one(&path, &origin, &host, pin_verdict(probe, strict, reported));
    }
}

/// 这一趟要判的几台：`(origin, host, 这一趟是否已严格校验, 它报过的逐地址指纹)` —— 目标一台，经跳板再加跳板那一台
/// （跳板的 origin / host 取请求里 `jump` 那一格：宿主按配置现查填的那一台原样配置；严格与否取 ack 的 `jump_strict`）。纯函数。
pub(crate) fn pin_targets<'a>(
    cfg: &RemoteConfig,
    req: &serde_json::Value,
    ack: &'a Ack,
) -> Vec<(
    String,
    String,
    bool,
    &'a std::collections::BTreeMap<String, String>,
)> {
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let mut out = vec![(
        cfg.origin_label(),
        cfg.host.clone(),
        ack.strict,
        &ack.fingerprints,
    )];
    if let Some(hop) = req.get("jump").filter(|j| j.is_object()) {
        let host = text(hop, "host").unwrap_or_default();
        let origin = text(hop, "label")
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| host.clone());
        out.push((origin, host, ack.jump_strict, &ack.jump_fingerprints));
    }
    out
}

fn settle_one(path: &std::path::Path, origin: &str, host: &str, verdict: PinVerdict) {
    let origin = origin.to_string();
    match verdict {
        PinVerdict::Pin(fp) => match pin_host_key_at(path, &origin, host, &fp) {
            Ok(PinWrite::Written) => notice(
                origin.clone(),
                HOST_KEY_PINNED,
                copy_text(
                    "rsDialHost.hostKey.pinned",
                    &[("origin", &origin), ("fingerprint", &fp)],
                ),
            ),
            Ok(other) => tracing::info!("[{origin}] 没有自动固化 host key：{other:?}"),
            Err(e) => tracing::warn!("[{origin}] 自动固化 host key 没写成：{e}"),
        },
        PinVerdict::Differs(all) => {
            let list = all
                .iter()
                .map(|(ep, fp)| format!("{ep} {fp}"))
                .collect::<Vec<_>>()
                .join(copy_text("rsDialHost.hostKey.listSep", &[]).as_str());
            notice(
                origin.clone(),
                HOST_KEY_DIFFERS,
                copy_text(
                    "rsDialHost.hostKey.differs",
                    &[("origin", &origin), ("list", &list)],
                ),
            );
        }
        PinVerdict::NoneReported => {
            tracing::info!("[{origin}] 本机后端没报逐地址指纹（旧版？）⇒ 不自动固化")
        }
        PinVerdict::NotAsked | PinVerdict::AlreadyStrict => {}
    }
}

// `open_stream`〔散文墓碑〕（`use:"stream"` 的一条 exec 字节流）删了：唯一的问者 `stream_source` 那个一次性 exec 原语随公钥推送进本机后端一起走了。

/// **一条到远端常驻后端监听口的隧道**（链路 `use:"tunnel"`：本机常驻后端在池里那条 SSH 连接上开
/// direct-tcpip 到远端 `127.0.0.1:port`）。出生带一次性总时限；接成流之后由调用方摘（`remote_resident::attach`）。
/// 失败回 `(说法, 开通道被远端回拒的原因码)`（`remote_resident::retry_tunnel` 据码分停 / 等）。
pub(crate) async fn tunnel(
    cfg: &RemoteConfig,
    port: u16,
) -> Result<DialStream, (Said, Option<String>)> {
    let req = request(cfg, "tunnel", serde_json::json!({ "tunnel_port": port }))
        .map_err(|e| (Said::from(e), None))?;
    open(cfg, &req, "tunnel", &mut |_| {}).await.map(|(s, _)| s)
}

/// **收全一条 exec**：stdout / stderr / 退出码。`abort_marker` 一出现就提前收（老后端掉进流模式永不 EOF）。
pub(crate) async fn capture(
    cfg: &RemoteConfig,
    cmd: &str,
    abort_marker: Option<&str>,
    max_bytes: usize,
) -> Result<RemoteExec, Said> {
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
        .map_err(link_said)?;
    Ok(RemoteExec {
        stdout: got.stdout,
        stderr: got.stderr,
        exit_status: got.exit_status,
    })
}

// 测试连接那一趟（短命探活、阶段行逐条交回）退役：拨号请求改由本机后端按界面交来的配置组（`dial/probe.rs`）。

// 端口转发那一形（`ForwardLink` · `forward`）退役：转发账连同开链路一起住本机常驻后端
//   （`src/backend/dial/forwards.rs`，查的是后端自己的可达表），界面经 `chan.call(<local>, "forward-*")` 直接问。

// ═══ 部署那条路：受限的远端文件一问一答（链路 `use:"files"`）═══════════════════
//
// 用户「SFTP 进本机常驻后端，只写暂存区」：界面进程零 SSH / 零 SFTP。自部署（F08 后端二进制 · `.build_id` ·
// `ccm` 入口）的**业务判定**留在 monitor（`sftp.rs`，一个判定函数都没动），
// 执行交给本机常驻后端那一份 SFTP（`src/backend/dial/sftp.rs`）—— 它**只许往 `~/.cc-monitor/staging/` 与
// `~/.cc-monitor/bin/` 写**（越界 ⇒ 应答 `code:"fenced"`，这里原话带回）。线上形状住后端那份头注与协议文档。

/// 一行应答的字节上限（`read` 的数据 base64 进这一行；部署读的都是小文件：标记 · 入口 · 脚本）。
fn files_reply_cap() -> u64 {
    8 * 1024 * 1024
}

/// `put` 之后等那一行应答的上限：后端先把字节收全（本机管道），再经 SFTP 传到远端（网络），再读回比对。
/// 一份 MB 级的后端二进制在慢链路上要好一会儿 —— 给宽，但有界。
const FILES_PUT_DEADLINE: Duration = Duration::from_secs(600);

/// 其余几问（`home` · `read` · `remove` · `mkdirs`）等应答的上限：各是一两个 SFTP 往返。
const FILES_ASK_DEADLINE: Duration = Duration::from_secs(60);

/// 读回来的那一份与期望的比对结论（后端算的）：`None` = 读不回来；`Some((读回长度, 首个差异))`。
pub(crate) type Readback = Option<(u64, Option<u64>)>;

/// 部署用的远端文件句柄：一条开在本机后端里的 `files` 链路。**只有这几问，写只许两处。**
///
/// 丢掉它 = 关链路 = 后端收掉那条 sftp 通道（那条 SSH 连接不跟着断）。
pub(crate) struct RemoteFs {
    link: tokio::sync::Mutex<DialStream>,
}

impl RemoteFs {
    /// 开一条 `files` 链路（拨号 / 池里复用 · 开 sftp 子系统），问一次起始目录（答得出 = 链路通了；值本身没人要）。
    pub(crate) async fn open(cfg: &RemoteConfig) -> Result<RemoteFs, Said> {
        let req = request(cfg, "files", serde_json::json!({}))?;
        let (link, _) = open(cfg, &req, "files", &mut |_| {})
            .await
            .map_err(|(e, _)| e)?;
        let fs = RemoteFs {
            // 长活：一次部署问好几次，**每一问**自带期限（[`FILES_ASK_DEADLINE`] / [`FILES_PUT_DEADLINE`]）。
            link: tokio::sync::Mutex::new(link.lives_long()),
        };
        let v = fs.ask(serde_json::json!({"op": "home"}), None).await?;
        v.get("home")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| Said::with_raw(copy_text("rsDialHost.open.noHome", &[]), &v))?;
        Ok(fs)
    }

    /// 一问一答。`bytes` 跟在请求行后面（只有 `put` 用）。失败那一形（`{"code","message","detail"}`）⇒ 那一句 ＋ 后端写的详情。
    async fn ask(
        &self,
        req: serde_json::Value,
        bytes: Option<&[u8]>,
    ) -> Result<serde_json::Value, Said> {
        use tokio::io::AsyncWriteExt;
        let deadline = if bytes.is_some() {
            FILES_PUT_DEADLINE
        } else {
            FILES_ASK_DEADLINE
        };
        let mut link = self.link.lock().await;
        let round = async {
            let mut line = req.to_string();
            line.push('\n');
            let sent = |e: std::io::Error| {
                Said::with_raw(
                    copy_text(
                        "rsDialHost.ask.sendBytesFailed",
                        &[("why", &copy_core::io_reason(e.kind()))],
                    ),
                    &e,
                )
            };
            link.write_all(line.as_bytes()).await.map_err(sent)?;
            if let Some(b) = bytes {
                link.write_all(b).await.map_err(sent)?;
            }
            link.flush().await.map_err(sent)?;
            ssh_link::reply_line(&mut link.r, files_reply_cap())
                .await
                .map_err(link_said)
        };
        let v = tokio::time::timeout(deadline, round).await.map_err(|_| {
            Said::from(copy_text(
                "rsDialHost.ask.timeout",
                &[("dur", &copy_core::format_elapsed(deadline))],
            ))
        })??;
        if let Some(code) = v.get("code").and_then(serde_json::Value::as_str) {
            let message = v
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let said = if code == "fenced" {
                copy_text(
                    "rsDialHost.ask.writeFenced",
                    &[("message", &message.to_string())],
                )
            } else {
                message.to_string()
            };
            let detail = v.get("detail").and_then(serde_json::Value::as_str);
            return Err(refused_said(said, detail));
        }
        Ok(v)
    }

    // 这里原先是 `stat` 那一问（落点那个文件在不在 / 多大）：落点那一份是谁改由本机常驻后端出计划时自己问（`deploy-plan`），
    //   monitor 这一侧零调用方 ⇒ 删了；链路那一侧的 `stat` 一问随之也删了。

    // `read` 那一问（整份读回一个小文件）零调用方了：唯一的读者是按目录取版本标记那条路（已退役）⇒ 删了。
    //   链路那一侧的 `read` 一问照旧在（`files` 链路协议没动）。

    /// 原子上传（EXCL 临时件 → 旧的改名 `.bak` → 上位 → 删 `.bak`；**绝不 setstat**）。`verify` ⇒ 后端读回比对，回结论。
    pub(crate) async fn put(
        &self,
        path: &str,
        bytes: &[u8],
        mode: u32,
        verify: bool,
    ) -> Result<Readback, Said> {
        let v = self
            .ask(
                serde_json::json!({"op": "put", "path": path, "size": bytes.len(), "mode": mode, "verify": verify}),
                Some(bytes),
            )
            .await?;
        let rb = v.get("readback").filter(|r| !r.is_null());
        Ok(rb.map(|r| {
            (
                r.get("len")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
                r.get("first_diff").and_then(serde_json::Value::as_u64),
            )
        }))
    }

    /// 删一份（不在 ⇒ `false`）。
    pub(crate) async fn remove(&self, path: &str) -> Result<bool, Said> {
        let v = self
            .ask(serde_json::json!({"op": "remove", "path": path}), None)
            .await?;
        Ok(v.get("removed").and_then(serde_json::Value::as_bool) == Some(true))
    }

    /// `mkdir -p`（每一级都过后端那道围栏）。
    pub(crate) async fn mkdirs(&self, path: &str) -> Result<(), Said> {
        self.ask(serde_json::json!({"op": "mkdirs", "path": path}), None)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/dial_host_tests.rs"]
mod tests;
