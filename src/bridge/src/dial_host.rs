//! 〔C2 · `设计/05 §13.2`〕**拨号代理的宿主**：定位本机后端二进制 · 把一台远端的配置翻成一份拨号请求 ·
//! 起 `<本机后端> --dial` 子进程 · 把它的两根管子交给通信层成员 [`crate::ssh_link`] 去读应答。
//!
//! # 它为什么不是通信层成员
//!
//! 它做的正是 `05 §2` 的 `C4`/`C5` 不许成员做的事：读环境变量与配置（`C4`）、起进程（`C5`）。
//! 与 `chan/host.rs`（绑回环造钥匙）、`local_backend_host.rs`（起本机后端）同一类 —— **宿主**。
//! 住顶层而不住 `backend/control/`：它要认目标平台（本机后端内嵌那一份是哪个 arch 的 Linux 二进制），
//! 而 `backend/` 那一半不许认平台（`the_backend_half_stays_platform_agnostic`）—— 与 `local_backend_host.rs` 同一个理由。
//!
//! # 🔴 没有退路（`D11`：「后端是给定的，不要退路」）
//!
//! 找不到本机后端二进制 ⇒ **报**，不再进程内拨 SSH。界面进程里的 `russh` 拨号除 SFTP 那一份
//! （`F7c` 独占的 `sftp.rs`，登记在 `inproc_dial.rs`）外全删了 —— 这里就是界面拿到一条 SSH 链路的**唯一**入口。
//!
//! # 进程形态（认下来的代价，`05 §13.4`）
//!
//! 一条链路一个代理子进程：远端长流常驻一个；一次性 exec / 测试连接起一个短命的；端口转发一条一个。
//! 子进程随 monitor 走（`Lifetime::JobKillOnClose`：丢掉句柄 = 收掉进程；Windows 上还有 Job 兜着）。

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::BufReader;

use crate::ssh_link::{self, Ack, ConnectStage, LinkError};
use crate::ssh_source::{RemoteConfig, RemoteExec};

/// 拨号代理二进制的**显式住址**（环境变量名）。开发树上 `externalBin` 不注入 ⇒ exe 旁边恒空，
/// 这个变量（或内嵌释放那一份）是开发树上的入口。
pub(crate) const DIAL_PROXY_ENV: &str = "CCM_DIAL_PROXY";

/// 装那份请求 JSON 的环境变量名 —— 与后端 `dial::REQUEST_ENV` 逐字相同（两侧各钉一半）。
///
/// **为什么走环境变量**：`argv` 在同机任何用户的 `ps` 里都看得见，`/proc/<pid>/environ` 只有本人读得到；
/// 也不走 stdin 第一行 —— 子进程的 stdin **纯粹**是那条链路，一个字节带外数据都没有。
pub(crate) const DIAL_REQUEST_ENV: &str = "CCM_DIAL_REQUEST";

/// 等代理回 ack 的上限：握手看门狗（黑洞地址 TCP 连上后握手可以无限阻塞）。与界面侧原来那条
/// `HANDSHAKE_DEADLINE` 同值。后端不许有 `timeout(`（`no_timer_guard`），⇒ 看门狗在这里执行：
/// 到点丢掉子进程句柄 = 收掉代理进程 = 关掉它手里那些 socket。
const ACK_DEADLINE: Duration = Duration::from_secs(45);

/// ack 之前每一行（阶段 / ack）的字节上限。ack 正常 < 300 字节，64 KiB 是两个数量级以上的余量；
/// 对端坏掉、或压根不是我们的代理时，一条没有换行的巨流不许变成无界堆分配。
fn ack_line_cap() -> u64 {
    64 * 1024
}

/// 解析拨号代理二进制。**只读、不写盘**。
///
/// 顺序：`CCM_DIAL_PROXY` → exe 旁（发版包）→ 本机后端自释放的那一份（开发构建内嵌时）。
/// 三处都没有 ⇒ `Err`，调用方**原样报出去**（`D11`）。
pub(crate) fn resolve_proxy() -> Result<PathBuf, String> {
    use crate::backend::control::local_backend::{self, Resolved};
    if let Some(raw) = std::env::var_os(DIAL_PROXY_ENV) {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "{DIAL_PROXY_ENV} 指向 {} —— 那不是一个文件（不猜别的路径）",
            p.display()
        ));
    }
    let triple = env!("CCM_TARGET_TRIPLE");
    if let Resolved::Found(p) = local_backend::resolve_beside_this_exe(triple) {
        return Ok(p);
    }
    // 开发构建：本机后端那条路（`local_backend_host::start_local_backend`）用的同一个落点与同一份解析。
    let extract_dir = dirs::home_dir()
        .map(|h| h.join(".cc-monitor").join("bin"))
        .unwrap_or_else(|| PathBuf::from("/tmp/.cc-monitor/bin"));
    let embedded = if cfg!(target_os = "linux") {
        crate::sftp::backend_binary(std::env::consts::ARCH).map(|d| (d.build_id, d.bytes))
    } else {
        None
    };
    match local_backend::resolve_or_extract(
        triple,
        &extract_dir,
        embedded,
        &crate::platform_fs::make_executable,
    ) {
        Resolved::Found(p) => Ok(p),
        Resolved::Missing { reason, looked_at } => Err(format!(
            "本机后端不在，远端连不了（远端的 SSH 由本机后端来拨）：{reason}；找过 {looked_at:?}。\
             开发树里设 {DIAL_PROXY_ENV} 指向一份 `cc-monitor-backend` 即可"
        )),
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

/// 一条**跑在拨号代理子进程里**的链路：读写两半都是那个子进程的管子。
///
/// 丢掉这个结构 = 丢掉子进程句柄 = 代理被收掉（`Lifetime::JobKillOnClose`）。
pub struct DialStream {
    _child: crate::spawn_managed::ManagedTokioChild,
    /// `None` = 已经半关（`shutdown` 把它丢了 ⇒ 子进程读到 EOF ⇒ 代理把 EOF 递给远端）。
    /// ⚠ **必须真的丢掉**：tokio 的 `ChildStdin::poll_shutdown` 不关管子，只靠它「关写半边」远端永远等不到 EOF。
    w: Option<tokio::process::ChildStdin>,
    /// ⚠ **必须是 `BufReader` 本体**：ack 那一行是按行读的，缓冲里很可能已经预读了后面的字节。
    r: BufReader<tokio::process::ChildStdout>,
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
        match self.w.as_mut() {
            Some(w) => tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(w), cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "写半边已经关了",
            ))),
        }
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.w.as_mut() {
            Some(w) => tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(w), cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if let Some(w) = self.w.as_mut() {
            std::task::ready!(tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(w), cx))?;
        }
        self.w = None; // 丢掉 ⇒ 管子关 ⇒ 代理读到 EOF
        std::task::Poll::Ready(Ok(()))
    }
}

/// 起代理、交请求、在 [`ACK_DEADLINE`] 内读完握手。成功 ⇒ 链路 ＋ ack。
///
/// 失败回 `(说法, 看到过的指纹)` —— 测试连接要把指纹给用户看（TOFU 固化 / 失配时比对）。
async fn open(
    cfg: &RemoteConfig,
    req: &serde_json::Value,
    want: &str,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<(DialStream, Ack), (String, Option<String>)> {
    let bin = resolve_proxy().map_err(|e| (e, None))?;
    // ★ F05 下半的那条埋点跟着拨号搬到这里：量的是「起代理进程 ＋ TCP ＋ 握手 ＋ 指纹校验 ＋ 鉴权 ＋ 开通道」
    //   —— 比原来多了一次进程创建，那正是 `设计/05 §13.4` 认下来的代价，这一行让它看得见。
    let t_handshake = std::time::Instant::now();
    let mut c = tokio::process::Command::new(&bin);
    c.arg("--dial")
        .env(DIAL_REQUEST_ENV, req.to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    // ★★ 三条策略（`00 §1.5.2`）：
    // · `Hidden` —— 拨号代理是个后台进程，绝不该在用户桌面上开窗。
    // · `JobKillOnClose` —— 界面退出 = 句柄 drop = 代理跟着走；Windows 上还进一个
    //   `KILL_ON_JOB_CLOSE` 的 Job ⇒ monitor 被强杀 / 崩溃时代理也跟着走。
    // · `Inherit` —— 代理的诊断（拨号失败原因、TOFU 警告）跟着界面进程的 stderr 走同一个地方。
    let mut child = crate::spawn_managed::spawn_managed_tokio(
        &mut c,
        crate::spawn_managed::ConsolePolicy::Hidden,
        crate::spawn_managed::Lifetime::JobKillOnClose,
        crate::spawn_managed::StderrSink::Inherit,
    )
    .map_err(|e| (format!("起拨号代理 {} 失败: {e}", bin.display()), None))?;
    let w = child
        .stdin
        .take()
        .ok_or_else(|| ("拨号代理没有 stdin 管子".to_string(), None))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ("拨号代理没有 stdout 管子".to_string(), None))?;
    let mut r = BufReader::new(stdout);
    let shake = ssh_link::handshake(&mut r, want, ack_line_cap(), on_stage);
    let ack = match tokio::time::timeout(ACK_DEADLINE, shake).await {
        Ok(Ok(ack)) => ack,
        Ok(Err(LinkError::Refused { why, fingerprint })) => {
            return Err((why, fingerprint));
        }
        Ok(Err(e)) => return Err((e.to_string(), None)),
        // 到点：`child` 随本函数返回被丢掉 ⇒ 代理被收掉。
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
        "[perf] ssh_source [{origin}] SSH 握手+鉴权 {}ms（经拨号代理：起进程＋TCP＋握手＋指纹校验＋auth＋开通道；\
         代理 {}；winner={:?}；指纹 {:?}）",
        t_handshake.elapsed().as_millis(),
        bin.display(),
        ack.endpoint,
        ack.fingerprint
    );
    Ok((
        DialStream {
            _child: child,
            w: Some(w),
            r,
        },
        ack,
    ))
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

/// 一条**端口转发**：代理那一侧绑好了本机回环口（`127.0.0.1:local_port`），每接进一条连接开一条隧道。
/// 丢掉它 = 代理被收掉 = 本地口释放、隧道全断。
pub struct ForwardLink {
    link: DialStream,
}

impl ForwardLink {
    /// 等下一条「接进了第 n 条连接」。`None` = 代理收工了（远端断了 / 它自己退了）。
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
