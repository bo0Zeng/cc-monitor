//! `--dial` 代理进程 —— **SSH 的一切都在这里**（`设计/05 §13`：拨 SSH 归后端）。
//!
//! # 它是什么
//!
//! monitor（界面进程）要跟某台远端说话时，不再自己拨 SSH：它起一个 `<本机后端> --dial`，
//! 把一份请求放进环境变量 `CCM_DIAL_REQUEST`，然后**只在这个进程的 stdin/stdout 上收发字节**。
//! 与远端跑 SSH 的每一件事都在本目录：多地址竞速（F45）· 跳板（F56）· host key 校验
//! （严格 / TOFU，回报实得指纹）· 鉴权（`key_path` 私钥文件 · ssh-agent）· keepalive ·
//! 开 channel 之后的三种用法（[`Use`]）。
//!
//! 🔴 **界面那一侧的 `russh` 拨号已经删了**（`设计/05 §13.4`，用户「后端是给定的，不要退路」）：
//! 没有「拿不到代理就进程内拨」那条回落，也没有「没配 `keyPath` 就进程内拨」那条回落 ——
//! 后者的根因（代理不会 ssh-agent）在本目录补上了。**唯一的例外**是 SFTP（`F7c` 独占的
//! `sftp.rs` 仍在进程内拨，登记在界面侧 `inproc_dial.rs`）。
//!
//! ⚠ **SFTP 子系统刻意不在本目录的用法里**：`readonly_guard` 的远端写那一层把
//! 「在 channel 上请求一个子系统」判作「远端文件传输能力」，红线 `I7` 的裁定（`ROADMAP.md#KU31`「远端 rc 能不能
//! 替用户写」）今天没拍 ⇒ 后端不许长出这条能力。SFTP 走代理要先过那一裁（`设计/05 §13.5`）。
//!
//! # 线上形状（**不是** `wire.rs` 那套协议 —— 那份一个字节没动）
//!
//! ```text
//! 界面 → 代理  环境变量 CCM_DIAL_REQUEST：一份 JSON = DialRequest
//!              stdin：stream 时**全部**是原始字节，原样写进 SSH channel；
//!                     capture 时不读；forward 时只等它 EOF（= 界面走了，收工）
//! 代理 → 界面  stdout：stages=true 时先有若干行 {"stage":{…}}（与界面 `ConnectStage` 同形）
//!                      然后**恰好一行** ack（DialAck，`\n` 结尾）
//!                      其后按用法：stream 原样字节 · capture 一行 Captured 后退出 ·
//!                      forward 每接进一条连接一行 {"accepted":n}
//! ```
//!
//! **为什么配置走环境变量而不走 argv / stdin 第一行**：`argv` 在同机任何用户的 `ps` 里都看得见；
//! stdin 第一行则要求界面往流里写带外字节，而界面那侧的写半边整个交给了 `inbound_client`。
//! 私钥**路径**、主机名、用户名不该躺在世界可读的地方；私钥**本体**从不过这条管子（凭据面 `K11`）。
//!
//! **为什么 ack 要有**：「连不上」与「连上了但远端还没说话」在管子上一模一样，ack 把它们分开。
//! **为什么 ack 带 `v` 与 `uses`**：老代理不认 `use` 字段，会把 `capture` 当成长流 ——
//! 界面据 `uses` 当场认出「这个代理太老」，而不是把一段原始字节当成 JSON 去解。
//!
//! # 进程形态（认下来的代价，`设计/05 §13.4`）
//!
//! 一条链路一个代理进程：远端长流常驻一个；一次性 exec 起一个短命的。它是界面的子进程，
//! 界面一退、管子一断，它就收工（Windows 上还有 Job 兜着）。
//! **更好的形状**（常驻的本机后端持有各远端的 SSH 连接、在已有那条管子上复用）要改 `wire.rs`，
//! 交用户拍板，本目录不做。
//!
//! # 边界（诚实登记）
//!
//! - **竞速不错开起拨**：本 crate 不许有「睡到点自己醒」的构件（`no_timer_guard`），
//!   RFC 8305 那 250 ms 的阶梯做不了 ⇒ 所有地址**同时**起拨，首个握手成功者胜、其余立即丢弃。
//!   界面给的顺序（last-good 排首）只决定同时完成时谁先被看到。
//! - **ssh-agent 的 Windows 那一半**（OpenSSH 命名管道）只在交叉编译上编得过，零真机读数。
//! - **musl 交叉编译没验**：门禁只做本机 gnu 构建。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

mod connect;
mod uses;

/// 装那份请求 JSON 的环境变量名。**两端各钉一半**，界面那边钉「写进这个名字」。
pub const REQUEST_ENV: &str = "CCM_DIAL_REQUEST";

/// 请求读不成 ⇒ 调用错误。与后端别处「一次性查询」的 `exit 2` 同族。
pub const EXIT_BAD_REQUEST: i32 = 2;
/// 请求读得懂但拨不通（TCP / 指纹 / 鉴权 / 开 channel 任一步失败）。
pub const EXIT_DIAL_FAILED: i32 = 3;

/// ack 里的协议版本。**v1** = 只有长流、只有一个地址、只会私钥文件（`K-P6b` 那一版）；
/// **v2** = 本文件（竞速 · 跳板 · agent · 三种用法 · 阶段行）。
pub const ACK_V: u32 = 2;

/// 本代理认得的用法 —— ack 的 `uses` 字段原样回这张表，界面据它判「代理够不够新」。
pub const USES: &[&str] = &["stream", "capture", "forward"];

/// 一个拨号地址。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// 跳板那一台（F56，单跳）。它自己的跳板字段由界面丢掉，不递归。
#[derive(Debug, Clone, Deserialize)]
pub struct JumpHop {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: Option<String>,
    pub host_key_fingerprint: Option<String>,
    /// 报错与阶段行里称呼它用的名字（界面那边的配置标签）。
    #[serde(default)]
    pub label: String,
}

/// 开出 channel 之后怎么用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Use {
    /// exec `command`，stdin/stdout 与 channel 原样对拷（后端长连接流 · 一次性查询）。
    #[default]
    Stream,
    /// exec `command`，收全 stdout / stderr / 退出码，印一行 [`Captured`] 后退出。
    Capture,
    /// 本进程绑本机回环口、每接进一条连接开一条 direct-tcpip（F58 端口转发）。
    Forward,
}

/// `use: capture` 的参数。
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureOpts {
    /// stdout / stderr 各自的收集上限（字节）。
    pub max_bytes: usize,
    /// stdout 一出现这个子串就收工（不认参数的老后端会掉进流模式、永不 EOF）。
    #[serde(default)]
    pub abort_marker: Option<String>,
}

/// `use: forward` 的参数。
#[derive(Debug, Clone, Deserialize)]
pub struct ForwardSpec {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// 界面交给代理的那份 JSON。
///
/// ⚠ **蛇形键**（不是界面配置那套 camelCase）：这条管子两端都是我们自己，
/// 而 `RemoteConfig` 的 camelCase 是**给前端看的**契约，两者不该被拴在一起。
/// ⚠ v1 那六个字段一个没改；v2 的字段全是可选的（老界面发来的请求照样读得动）。
#[derive(Debug, Clone, Deserialize)]
pub struct DialRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// OpenSSH 格式私钥的**路径**（不是私钥本体）。缺席 = 走 ssh-agent。
    pub key_path: Option<String>,
    /// 期望的 host key 指纹（`SHA256:…`）。`None` = TOFU 接受并 `warn`。
    pub host_key_fingerprint: Option<String>,
    /// 要 exec 的命令行（`stream` / `capture`；界面已 `shell_quote` 过；`forward` 不用它）。
    #[serde(default)]
    pub command: String,
    /// 竞速的地址顺序（界面已按 last-good 排好）。空 = 只有 `host:port`。
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    /// 经这台跳板（fail-closed：它连不上就报错，不回落直连）。
    #[serde(default)]
    pub jump: Option<JumpHop>,
    #[serde(default, rename = "use")]
    pub use_: Use,
    #[serde(default)]
    pub capture: Option<CaptureOpts>,
    #[serde(default)]
    pub forward: Option<ForwardSpec>,
    /// ack 之前逐行报阶段（测试连接那六格）。
    #[serde(default)]
    pub stages: bool,
    /// 短命探活（inactivity 拆链）而不是长连接（keepalive 保活）。
    #[serde(default)]
    pub probe: bool,
}

impl DialRequest {
    /// 竞速顺序：给了就用给的，没给就只有 `host:port`。
    pub fn race_order(&self) -> Vec<Endpoint> {
        if self.endpoints.is_empty() {
            vec![Endpoint {
                host: self.host.clone(),
                port: self.port,
            }]
        } else {
            self.endpoints.clone()
        }
    }
}

/// 代理回给界面的那一行 JSON。
#[derive(Debug, Clone, Serialize)]
pub struct DialAck {
    pub ok: bool,
    /// `ok=false` 时的人话原因。界面把它原样冒泡给重连那一层。
    pub error: Option<String>,
    /// 实际观察到的 host key 指纹 —— 界面侧 TOFU 固化要它（失败时也尽量带上）。
    pub fingerprint: Option<String>,
    /// 竞速胜出的地址（`host:port`）—— 界面记 last-good、测试连接展示「你正连着哪条路」。
    pub endpoint: Option<String>,
    /// 协议版本（[`ACK_V`]）。
    pub v: u32,
    /// 本代理认得的用法（[`USES`]）。
    pub uses: &'static [&'static str],
}

impl DialAck {
    fn failed(error: String, fingerprint: Option<String>) -> Self {
        DialAck {
            ok: false,
            error: Some(error),
            fingerprint,
            endpoint: None,
            v: ACK_V,
            uses: USES,
        }
    }
}

/// 一行阶段（`stages=true` 时在 ack 之前出）。**形状与界面 `ssh_source::ConnectStage` 逐字段相同** ——
/// 界面那一侧原样反序列化进它自己的类型，对拍判据住界面那侧。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Stage {
    Dialing {
        endpoint: String,
    },
    HostKey {
        endpoint: String,
        fingerprint: String,
    },
    Failed {
        endpoint: String,
        reason: String,
    },
    Won {
        endpoint: String,
    },
    Auth {
        ok: bool,
        detail: Option<String>,
    },
    Established,
}

/// `use: capture` 的结果行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status（连接被掐 / 服务端不守规矩）。**不许把 `None` 当成 0**。
    pub exit_status: Option<u32>,
}

/// 阶段行的出口。`stages=false` 时一个字节都不写。
///
/// 阶段是从握手的好几个并发任务里产生的，而 stdout 只有一个 ⇒ 先进一个共享的缓冲，
/// ack 之前按到达顺序一次写出去（阶段只在 ack 之前有意义）。
#[derive(Clone, Default)]
pub(crate) struct StageSink {
    on: bool,
    buf: Arc<Mutex<Vec<Stage>>>,
}

impl StageSink {
    pub(crate) fn new(on: bool) -> Self {
        StageSink {
            on,
            buf: Arc::default(),
        }
    }

    pub(crate) fn emit(&self, s: Stage) {
        if self.on {
            self.buf.lock().unwrap_or_else(|e| e.into_inner()).push(s);
        }
    }

    fn drain(&self) -> Vec<Stage> {
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// 解析那份请求 JSON。**抽出来是为了判据够得着它** —— 判据不该去起一个真进程
/// 才能验「蛇形键读得动」。
pub(crate) fn parse_request(raw: &str) -> Result<DialRequest, serde_json::Error> {
    serde_json::from_str(raw.trim())
}

/// 写一行 JSON 并 flush。**必须 flush** —— 界面在有界读行上等着它。
async fn write_line<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    w: &mut W,
    v: &T,
) -> std::io::Result<()> {
    let mut line = serde_json::to_string(v).map_err(std::io::Error::other)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

/// 把一行 ack 写出去并 flush。
async fn write_ack<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    ack: &DialAck,
) -> std::io::Result<()> {
    write_line(w, ack).await
}

/// 先写攒下的阶段行，再写 ack。
async fn write_stages_then_ack<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    stages: &StageSink,
    ack: &DialAck,
) -> std::io::Result<()> {
    for s in stages.drain() {
        write_line(w, &serde_json::json!({ "stage": s })).await?;
    }
    write_ack(w, ack).await
}

/// `--dial` 那条分派臂的实现。**不读 argv** —— 参数只用来在诊断里回显。
pub async fn run(args: &[String]) -> i32 {
    tracing::info!("dial: 代理进程起来了（argv={args:?}）");
    let mut out = tokio::io::stdout();

    let raw = match std::env::var(REQUEST_ENV) {
        Ok(s) if !s.trim().is_empty() => s,
        _ => {
            tracing::error!("dial: 环境变量 {REQUEST_ENV} 没设（或是空的）—— 界面没交请求");
            return EXIT_BAD_REQUEST;
        }
    };
    let req: DialRequest = match parse_request(&raw) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("dial: {REQUEST_ENV} 不是一个合法的 DialRequest: {e}");
            let _ = write_ack(
                &mut out,
                &DialAck::failed(format!("请求解析失败: {e}"), None),
            )
            .await;
            return EXIT_BAD_REQUEST;
        }
    };

    let stages = StageSink::new(req.stages);
    let linked = match connect::establish(&req, &stages).await {
        Ok(l) => l,
        Err((e, fp)) => {
            tracing::error!("dial: 拨号失败: {e}");
            let _ = write_stages_then_ack(&mut out, &stages, &DialAck::failed(e, fp)).await;
            return EXIT_DIAL_FAILED;
        }
    };
    uses::serve(&req, linked, &stages, &mut out).await
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
