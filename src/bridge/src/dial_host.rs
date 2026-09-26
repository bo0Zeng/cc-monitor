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

use crate::copy_table::copy_text;
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

/// 〔NT2 · A4〕**一次性那一趟的总时限**：开链路 → 握手（其中握手另受 [`ACK_DEADLINE`]）→ 远端跑 → 读完，
/// 从开链路那一刻起算**一个绝对时刻**（`设计/05 §3.3.2`：一次调用一个绝对时刻，不是每跳一个 `Duration`）。
///
/// 为什么非有不可（`设计/15 §3.2` 第 4 条红线「先装期限再复用」）：连接复用之后，一条卡住的一次性查询占着
/// 池里那条共享连接的一格、永不释放 —— 局部卡死升级成全局卡死。后端零定时器（`no_timer_guard`）⇒ 期限只能在
/// 调用方这一侧执行；到点 ⇒ 读写报 `TimedOut` ⇒ 调用方返回、丢掉链路 ⇒ `link-close` ⇒ 后端收掉那个任务、格还回去。
///
/// **默认有，豁免要点名**：[`open`] 开出来的每一条链路出生就带着它；本来就该长活的三形
/// （后端长连接流 · 端口转发 · 部署文件面 —— 后者每一问自带期限）显式调 [`DialStream::lives_long`] 摘掉，
/// 那三处由 `dial_host_tests::only_the_three_long_lived_links_drop_the_deadline` 两向钉住。
///
/// 值：今天各调用方外面套的最宽是「握手 45 s ＋ 读 30 s」、`acct_iso_deploy` 整趟 45 s ⇒ 120 s 不收紧任何一条既有的；
/// 它是**天花板**（调用方外面再套的更短期限照旧先到）。⚠ `05 §3.3.2`「值归后端」这一格没做到（与 [`ACK_DEADLINE`] 同住这里）。
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

/// 〔SR1b〕同上，但问的是**哪一条命令**：本机后端那条流在、且认 `cmd` ⇒ 回它的客户端。
/// 不在 ⇒ 报「本机后端不在」；不认 ⇒ 报「本机后端太旧」。传输台的中继（`sftp_pool.rs`）也从这里拿。
pub(crate) async fn local_backend_accepting(cmd: &str) -> Result<Arc<InboundClient>, String> {
    let local = inbound_client::LOCAL_ORIGIN;
    for attempt in 0..LOCAL_WAIT_TRIES {
        if let Some(c) = inbound_client::client_for(local) {
            if !c.accepts(cmd) {
                return Err(copy_text("rsDialHost.local.tooOld", &[]));
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
        "host_key_fingerprint": effective_fingerprint(cfg),
        "endpoints": endpoints,
        "use": use_,
        "agent_sock": agent_sock(),
    });
    if let Some(jump_label) = cfg.jump.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if jump_label == origin {
            return Err(copy_text("rsDialHost.jump.loop", &[]));
        }
        let jump_cfg = crate::load_remote_config_by_label(jump_label).ok_or_else(|| {
            copy_text(
                "rsDialHost.jump.notFound",
                &[("jumpLabel", &jump_label.to_string())],
            )
        })?;
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

/// 〔NT2 · A4〕总时限落在链路读写上的那一层：到点之后每一次读 / 写都报 `TimedOut`，不再碰里面那条链路。
///
/// 它是**一次性的上界**（与 `tokio::time::timeout_at` 同一件事），不是节拍：到点那一刻叫醒一次等着的读写，此后不再醒。
/// 关写半边（`shutdown` = 关链路）不受它管 —— 到点之后调用方照样要能把链路关掉。
pub(crate) struct Bounded<S> {
    inner: S,
    /// 到点那一刻 ＋ 用来说话的几样。`None` = 不设（长活那三形）。
    due: Option<Due>,
}

/// 〔W5-VIS · `设计/05 §3.3.2`「一个预算、多个归因点」〕期限只有一个，但到点时要说得出**卡在哪一段**：
/// 同一个数盖着「握手」与「远端跑」两段（`设计/15 §3.6` 小病：弱网上握手慢一点就被判查询超时，而两种成因处置完全不同）。
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

    /// 〔W5-VIS〕握手（开链路 ＋ 拨号 ＋ 鉴权 ＋ ack）做完了：记下这一刻，到点时据此把总时限拆成两段说。
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

/// 〔W5-VIS · `设计/05 §3.3.2`〕到点那句话：**一个预算、按段归因**。纯函数（判据直接喂时长，不睡墙钟）。
/// `shaken` = 起算之后多久握完手（`None` = 到点时还在握手）；`waited` = 起算到此刻。
pub(crate) fn expiry_note(total: Duration, shaken: Option<Duration>, waited: Duration) -> String {
    let secs = total.as_secs().to_string();
    match shaken {
        None => copy_text("rsDialHost.deadline.expiredInShake", &[("secs", &secs)]),
        Some(shake) => copy_text(
            "rsDialHost.deadline.expired",
            &[
                ("secs", &secs),
                ("shake", &format!("{:.1}", shake.as_secs_f64())),
                (
                    "run",
                    &format!("{:.1}", waited.saturating_sub(shake).as_secs_f64()),
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

/// 一条**开在本机后端里**的链路（〔SR1a〕C2 那一版是一个 `--dial` 子进程的两根管子）。
///
/// 丢掉这个结构 = 关链路（`link-close`）= 后端收掉它的拨号 / 服务任务；那条 SSH 连接**不跟着断**
/// （同一台远端的别的链路可能还在用它）。
///
/// 〔NT2 · A4〕出生就带着 [`ONE_SHOT_DEADLINE`]（[`open`] 起算）；长活那三形调 [`DialStream::lives_long`] 摘掉。
pub struct DialStream {
    /// ⚠ **必须是 `BufReader` 本体**：ack 那一行是按行读的，缓冲里很可能已经预读了后面的字节。
    /// 〔NT2〕期限那一层垫在 `BufReader` 底下 ⇒ 握手那几行、收全那一行、调用方自己的读写全在同一个时限里。
    r: BufReader<Bounded<LinkStream>>,
}

impl DialStream {
    /// 〔NT2 · A4〕**摘掉总时限**：只给本来就该长活的那三形（后端长连接流 · 端口转发 · 部署文件面）。
    /// 调用点由 `dial_host_tests::only_the_three_long_lived_links_drop_the_deadline` 两向钉住 ——
    /// 多一处 = 又有一条一次性的路没有总时限（`15 §3.2` 第 4 条红线）。
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
/// 失败回 `(说法, 看到过的指纹)` —— 测试连接要把指纹给用户看（TOFU 固化 / 失配时比对）。
async fn open(
    cfg: &RemoteConfig,
    req: &serde_json::Value,
    want: &str,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<(DialStream, Ack), (String, Option<String>)> {
    // 〔NT2 · A4〕这一趟的总时限从这一刻起算（开链路本身也在里面）。
    let due = tokio::time::Instant::now() + ONE_SHOT_DEADLINE;
    let client = local_channel().await.map_err(|e| (e, None))?;
    // ★ F05 下半的那条埋点跟着拨号搬到这里：量的是「开链路 ＋（池里没有时）TCP ＋ 握手 ＋ 指纹校验 ＋ 鉴权 ＋ 开通道」。
    //   〔SR1a〕同一台远端已经有连接时，这个数只剩「开一条 channel」—— 复用的收益就在这一行里看得见。
    let t_handshake = std::time::Instant::now();
    let link = LinkStream::open(client, req.clone(), LINK_CALL_BUDGET)
        .await
        .map_err(|e| (e, None))?;
    let mut r = BufReader::new(Bounded::new(link, Some((due, ONE_SHOT_DEADLINE))));
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
                copy_text(
                    "rsDialHost.open.timeout",
                    &[
                        ("secs", &(ACK_DEADLINE.as_secs()).to_string()),
                        (
                            "addr",
                            &(cfg
                                .endpoints()
                                .iter()
                                .map(|e| format!("{}:{}", e.host, e.port))
                                .collect::<Vec<_>>()
                                .join(", "))
                            .to_string(),
                        ),
                    ],
                ),
                None,
            ))
        }
    };
    // 〔W5-VIS〕握手做完了 ⇒ 记下这一刻：之后若总时限到点，那句话说得出「握手用了多久、远端跑了多久」。
    r.get_mut().mark_shaken();
    // 〔VIS2 · `设计/15 §3.4 ①`〕ack 成功 = 后端那边鉴权已过 ⇒ 判要不要自动固化。
    settle_host_key(cfg, req, &ack);
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

// ═══ 〔VIS2 · `设计/15 §3.4 ①`「自动固化 ＋ 默认转严格 ＋ 保住多地址那一格」〕host key 自动固化 ═══════════════

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

/// 🔴 判定只此一处。`configured` = 这一趟请求里交的指纹。
pub(crate) fn pin_verdict(
    probe: bool,
    configured: Option<&str>,
    reported: &std::collections::BTreeMap<String, String>,
) -> PinVerdict {
    if probe {
        return PinVerdict::NotAsked;
    }
    if configured.is_some_and(|f| !f.trim().is_empty()) {
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

/// 往 `config.json` 的 `remote.hosts` 里那一台写指纹：现读 → 只改那一台的 `hostKeyFingerprint` → 经补丁口
/// （`config::patch_config_at`）交 `remote.hosts` 一条。⚠ 补丁口够不着数组元素 ⇒ 整列是锁外算的（报备过）。
pub(crate) fn pin_host_key_at(
    path: &std::path::Path,
    origin: &str,
    host: &str,
    fp: &str,
) -> Result<PinWrite, String> {
    use serde_json::Value;
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let root: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let Some(hosts) = root.pointer("/remote/hosts").and_then(Value::as_array) else {
        return Ok(PinWrite::NotFound);
    };
    let field = |h: &Value, k: &str| {
        h.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    // origin 的口径同 `lib.rs::parse_host_obj`：`label` 非空取它，否则 `host`。
    let hits: Vec<usize> = (0..hosts.len())
        .filter(|&i| {
            field(&hosts[i], "label")
                .or_else(|| field(&hosts[i], "host"))
                .as_deref()
                == Some(origin)
                && field(&hosts[i], "host").as_deref() == Some(host)
        })
        .collect();
    let [i] = hits[..] else {
        return Ok(if hits.is_empty() {
            PinWrite::NotFound
        } else {
            PinWrite::Ambiguous
        });
    };
    if field(&hosts[i], "hostKeyFingerprint").is_some_and(|f| !f.trim().is_empty()) {
        return Ok(PinWrite::AlreadySet);
    }
    let mut next = hosts.clone();
    next[i]["hostKeyFingerprint"] = Value::String(fp.to_string());
    crate::config::patch_config_at(
        path,
        &[crate::config::ConfigEdit::Set {
            path: vec!["remote".into(), "hosts".into()],
            value: Value::Array(next),
        }],
    )
    .map_err(|e| e.to_string())?;
    Ok(PinWrite::Written)
}

/// 〔VIS2 · 默认转严格〕这一趟交给后端的指纹：`cfg` 里有就用；没有 ⇒ 盘上同一台（origin 与 host 都相同）的。
/// `ssh_source::run` 手里那份 `cfg` 是起来时读的 —— 不现读的话，固化之后它的每次重连照旧 TOFU 到重启。
fn effective_fingerprint(cfg: &RemoteConfig) -> Option<String> {
    effective_fingerprint_in(cfg, || {
        crate::load_remote_config_by_label(&cfg.origin_label())
    })
}

pub(crate) fn effective_fingerprint_in(
    cfg: &RemoteConfig,
    on_disk: impl FnOnce() -> Option<RemoteConfig>,
) -> Option<String> {
    let set = |f: Option<&String>| f.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    set(cfg.host_key_fingerprint.as_ref()).or_else(|| {
        on_disk()
            .filter(|d| d.host == cfg.host)
            .and_then(|d| set(d.host_key_fingerprint.as_ref()))
    })
}

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

/// 成功拨号之后：判 → 固化 / 说出来。
fn settle_host_key(cfg: &RemoteConfig, req: &serde_json::Value, ack: &Ack) {
    let probe = req.get("probe").and_then(serde_json::Value::as_bool) == Some(true);
    let configured = req
        .get("host_key_fingerprint")
        .and_then(serde_json::Value::as_str);
    let origin = cfg.origin_label();
    match pin_verdict(probe, configured, &ack.fingerprints) {
        PinVerdict::Pin(fp) => {
            let Some(path) = crate::paths::resolve_config_path() else {
                return;
            };
            match pin_host_key_at(&path, &origin, &cfg.host, &fp) {
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
            }
        }
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
    let cfg = crate::load_remote_config_by_label(origin).ok_or_else(|| {
        copy_text(
            "rsDialHost.forward.noConfig",
            &[("machine", &origin.to_string())],
        )
    })?;
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
        .map_err(|(e, _)| {
            copy_text(
                "rsDialHost.forward.connectFailed",
                &[("machine", &origin.to_string()), ("e", &e.to_string())],
            )
        })?;
    // 〔NT2〕长活：用户开着就一直在，关了（丢 `ForwardLink`）就收。
    Ok(ForwardLink {
        link: link.lives_long(),
    })
}

// ═══ 〔SR1b · 2026-09-24〕部署那条路：受限的远端文件一问一答（链路 `use:"files"`）═══════════════════
//
// 用户 V89「SFTP 进本机常驻后端，只写暂存区」：界面进程零 SSH / 零 SFTP。自部署（F08 后端二进制 · `.build_id` ·
// `ccm` 入口 · cc-acct-iso）的**业务判定**留在 monitor（`sftp.rs` / `acct_iso_deploy.rs`，一个判定函数都没动），
// 执行交给本机常驻后端那一份 SFTP（`src/backend/dial/sftp.rs`）—— 它**只许往 `~/.cc-monitor/staging/` 与
// `~/.cc-monitor/bin/` 写**（越界 ⇒ 应答 `code:"fenced"`，这里原话带回）。线上形状住后端那份头注与协议文档。

/// 一行应答的字节上限（`read` 的数据 base64 进这一行；部署读的都是小文件：标记 · 入口 · 脚本）。
fn files_reply_cap() -> u64 {
    8 * 1024 * 1024
}

/// `put` 之后等那一行应答的上限：后端先把字节收全（本机管道），再经 SFTP 传到远端（网络），再读回比对。
/// 一份 MB 级的后端二进制在慢链路上要好一会儿 —— 给宽，但有界。
const FILES_PUT_DEADLINE: Duration = Duration::from_secs(600);

/// 其余几问（`home` · `stat` · `read` · `remove` · `mkdirs`）等应答的上限：各是一两个 SFTP 往返。
const FILES_ASK_DEADLINE: Duration = Duration::from_secs(60);

/// 读回来的那一份与期望的比对结论（后端算的）：`None` = 读不回来；`Some((读回长度, 首个差异))`。
pub(crate) type Readback = Option<(u64, Option<u64>)>;

/// 部署用的远端文件句柄：一条开在本机后端里的 `files` 链路。**只有这几问，写只许两处。**
///
/// 丢掉它 = 关链路 = 后端收掉那条 sftp 通道（那条 SSH 连接不跟着断）。
pub(crate) struct RemoteFs {
    link: tokio::sync::Mutex<DialStream>,
    home: String,
}

impl RemoteFs {
    /// 开一条 `files` 链路（拨号 / 池里复用 · 开 sftp 子系统），问一次起始目录。
    pub(crate) async fn open(cfg: &RemoteConfig) -> Result<RemoteFs, String> {
        let req = request(cfg, "files", serde_json::json!({}))?;
        let (link, _) = open(cfg, &req, "files", &mut |_| {})
            .await
            .map_err(|(e, _)| e)?;
        let mut fs = RemoteFs {
            // 〔NT2〕长活：一次部署问好几次，**每一问**自带期限（[`FILES_ASK_DEADLINE`] / [`FILES_PUT_DEADLINE`]）。
            link: tokio::sync::Mutex::new(link.lives_long()),
            home: String::new(),
        };
        let v = fs.ask(serde_json::json!({"op": "home"}), None).await?;
        fs.home = v
            .get("home")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| copy_text("rsDialHost.open.noHome", &[("v", &v.to_string())]))?
            .to_string();
        Ok(fs)
    }

    /// 远端 SFTP 的起始目录（真路径）。
    pub(crate) fn home(&self) -> &str {
        &self.home
    }

    /// 一问一答。`bytes` 跟在请求行后面（只有 `put` 用）。失败那一形（`{"code","message"}`）⇒ `Err(message)`。
    async fn ask(
        &self,
        req: serde_json::Value,
        bytes: Option<&[u8]>,
    ) -> Result<serde_json::Value, String> {
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
            link.write_all(line.as_bytes()).await.map_err(|e| {
                copy_text("rsDialHost.ask.sendRequestFailed", &[("e", &e.to_string())])
            })?;
            if let Some(b) = bytes {
                link.write_all(b).await.map_err(|e| {
                    copy_text("rsDialHost.ask.sendBytesFailed", &[("e", &e.to_string())])
                })?;
            }
            link.flush().await.map_err(|e| {
                copy_text("rsDialHost.ask.sendRequestFailed", &[("e", &e.to_string())])
            })?;
            ssh_link::reply_line(&mut link.r, files_reply_cap())
                .await
                .map_err(|e| e.to_string())
        };
        let v = tokio::time::timeout(deadline, round).await.map_err(|_| {
            copy_text(
                "rsDialHost.ask.timeout",
                &[("secs", &(deadline.as_secs()).to_string())],
            )
        })??;
        if let Some(code) = v.get("code").and_then(serde_json::Value::as_str) {
            let message = v
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            return Err(if code == "fenced" {
                copy_text(
                    "rsDialHost.ask.writeFenced",
                    &[("message", &message.to_string())],
                )
            } else {
                message.to_string()
            });
        }
        Ok(v)
    }

    /// 那个文件在不在 / 多大：`(metadata 的 size, 补问的 exists)` —— 与 `sftp::interpret_target_probe` 入参同形。
    pub(crate) async fn stat(
        &self,
        path: &str,
    ) -> Result<(Option<Option<u64>>, Option<bool>), String> {
        let v = self
            .ask(serde_json::json!({"op": "stat", "path": path}), None)
            .await?;
        let meta = v
            .get("meta")
            .filter(|m| !m.is_null())
            .map(|m| m.get("size").and_then(serde_json::Value::as_u64));
        Ok((meta, v.get("exists").and_then(serde_json::Value::as_bool)))
    }

    /// 整份读回来：`(字节, 读不出时补问的 exists, 读到空时补问的 size)`。
    pub(crate) async fn read(
        &self,
        path: &str,
        max: u64,
    ) -> Result<(Option<Vec<u8>>, Option<bool>, Option<u64>), String> {
        let v = self
            .ask(
                serde_json::json!({"op": "read", "path": path, "max": max}),
                None,
            )
            .await?;
        let data = match v.get("data").and_then(serde_json::Value::as_str) {
            Some(t) => Some(crate::link_mux::b64_decode(t)?),
            None => None,
        };
        Ok((
            data,
            v.get("exists").and_then(serde_json::Value::as_bool),
            v.get("size").and_then(serde_json::Value::as_u64),
        ))
    }

    /// 原子上传（EXCL 临时件 → 旧的改名 `.bak` → 上位 → 删 `.bak`；**绝不 setstat**）。`verify` ⇒ 后端读回比对，回结论。
    pub(crate) async fn put(
        &self,
        path: &str,
        bytes: &[u8],
        mode: u32,
        verify: bool,
    ) -> Result<Readback, String> {
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
    pub(crate) async fn remove(&self, path: &str) -> Result<bool, String> {
        let v = self
            .ask(serde_json::json!({"op": "remove", "path": path}), None)
            .await?;
        Ok(v.get("removed").and_then(serde_json::Value::as_bool) == Some(true))
    }

    /// `mkdir -p`（每一级都过后端那道围栏）。
    pub(crate) async fn mkdirs(&self, path: &str) -> Result<(), String> {
        self.ask(serde_json::json!({"op": "mkdirs", "path": path}), None)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/dial_host_tests.rs"]
mod tests;
