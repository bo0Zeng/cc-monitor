//! **SSH 的一切都在这里**（`设计/05 §13`：拨 SSH 归后端）—— 〔SR1a〕而且只在**本机那一个常驻后端**里。
//!
//! # 它是什么
//!
//! 〔SR1a · 2026-09-24，用户裁「改成单一常驻后端」〕本机只常驻一个后端；到各远端的 SSH 连接
//! **由它持有、按拨号身份复用**（[`pool`]）。monitor（界面进程）要跟某台远端说话时，经它与本机后端之间
//! **那条已有的流**开一条「链路」（`link-open`，[`link`]），把一份拨号请求（[`DialRequest`]）交过来，
//! 然后**只在这条链路上收发字节**。与远端跑 SSH 的每一件事都在本目录：多地址竞速（F45）· 跳板（F56）·
//! host key 校验（严格 / TOFU，回报实得指纹）· 鉴权（`key_path` 私钥文件 · ssh-agent）· keepalive ·
//! 开 channel 之后的三种用法（[`Use`]）。
//!
//! 〔墓碑 —— C2 那一版的原话要点：「monitor 起一个 `<本机后端> --dial`，把请求放进环境变量
//!  `CCM_DIAL_REQUEST`，然后只在这个进程的 stdin/stdout 上收发字节」「一条链路一个代理进程」。
//!  SR1a 之后这两句都不成立：`--dial` 那条分派臂删了，每链路一个进程的形态退场。〕
//!
//! 🔴 **界面那一侧的 `russh` 拨号已经删了**（`设计/05 §13.4`，用户「后端是给定的，不要退路」）：
//! 没有「拿不到常驻后端就进程内拨」那条回落，也没有「起一个一次性代理进程」那条回落 ——
//! 常驻后端不在 ⇒ monitor **报**（`dial_host.rs`）。
//!
//! 〔SR1b · 2026-09-24，用户 V89「SFTP 进本机常驻后端，只写暂存区」〕**SFTP 也在这里了**（[`sftp`]）：
//! 在池里那条连接上开 sftp 子系统，远端写**只许两处**（`~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`）。
//! 界面拿不到原始 SFTP 字节（`use:"subsystem"` 照旧回 `unsupported_use`，见 [`link`]）—— 它只有两条路：
//! 部署走链路 `use:"files"`（受限的远端文件一问一答）；传输走入方向命令 `transfer-*`（`control/transfer.rs`）。
//! 〔墓碑 —— SR1a 那一版原话要点：「SFTP 子系统刻意不在本目录的用法里……红线 `I7` 的裁定今天没拍 ⇒ 后端不许长出
//!  这条能力」。V89 拍了；`INVARIANTS §41.6` 的 V89 订正与 `readonly_guard::remote_write_layer` 同拍改写。〕
//!
//! # 一条链路上的字节（与 C2 拨号代理的 stdout **逐字节同形**，线上帧见 `wire.rs` 的 `LinkData`）
//!
//! ```text
//! 界面 → 后端  link-open：{"link","window","dial":DialRequest}
//!              link-data：stream 时是原样写进 SSH channel 的字节；capture 时不读；forward 时不读
//!              link-close：界面走了，收工
//! 后端 → 界面  stages=true 时先有若干行 {"stage":{…}}（与界面 `ConnectStage` 同形）
//!              然后**恰好一行** ack（DialAck，`\n` 结尾）
//!              其后按用法：stream 原样字节 · capture 一行 Captured 后结束 ·
//!              forward 每接进一条连接一行 {"accepted":n} ·
//!              〔SR1b〕files 之后一问一答（上行一行请求、下行一行应答，`sftp.rs` 头注）
//! ```
//!
//! **为什么 ack 要有**：「连不上」与「连上了但远端还没说话」在链路上一模一样，ack 把它们分开。
//! **为什么 ack 带 `v` 与 `uses`**：界面据 `uses` 当场认出「这个后端不认这种用法」，
//! 而不是把一段原始字节当成 JSON 去解。
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
pub mod link;
mod pool;
pub(crate) mod sftp;
pub(crate) mod uses;

/// ack 里的协议版本。**v1** = 只有长流、只有一个地址、只会私钥文件（`K-P6b` 那一版）；
/// **v2** = 本文件（竞速 · 跳板 · agent · 三种用法 · 阶段行）。
/// 〔SR1b〕多一种用法 `files` **不 bump 它**：老界面不发 `files`，新界面凭 `uses` 认出老后端（`TooOld`），
/// 版本号只在「同一种用法的字节形状变了」时才动。
pub const ACK_V: u32 = 2;

/// 本代理认得的用法 —— ack 的 `uses` 字段原样回这张表，界面据它判「代理够不够新」。
pub const USES: &[&str] = &["stream", "capture", "forward", "files"];

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
    /// 〔SR1b〕开 sftp 子系统，之后在链路上一问一答（受限的远端文件操作：写只许两处，[`sftp`]）。
    Files,
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
    /// 〔SR1a〕ssh-agent 套接字的路径（Unix；界面进程**此刻**的 `SSH_AUTH_SOCK`）。
    ///
    /// 为什么要界面交过来而不读本进程的环境：常驻后端**活得比任何一个界面进程都长**（脱离、跨界面重启），
    /// 它身上那份 `SSH_AUTH_SOCK` 是**第一个**起它的界面给的 —— 用户重新登录之后 agent 换了路径，
    /// 读自己的环境就会连一个已经不在的套接字。缺席 = 用本进程自己的（C2 那一版的行为）。
    /// 它是一条路径、不是凭据本体（凭据面 `K11`）；它**不进连接池的身份**（换 agent 不该换连接）。
    #[serde(default)]
    pub agent_sock: Option<String>,
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
    /// 〔VIS2 · `设计/15 §3.4 ①`「保住多地址那一格」〕这一趟报过指纹的每条地址 → 指纹。additive（`ACK_V` 不动）；
    /// 界面只在各地址一致时自动固化。失败的 ack 恒空（指纹照旧在 `fingerprint`）。
    pub fingerprints: std::collections::BTreeMap<String, String>,
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
            fingerprints: Default::default(),
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

/// 〔SR1a〕解析 `link-open` 的 `dial` 字段（C2 那一版从环境变量读同一份 JSON）。
/// **抽出来是为了判据够得着它** —— 判据不该去开一条真链路才能验「蛇形键读得动」。
pub(crate) fn parse_request_value(v: &serde_json::Value) -> Result<DialRequest, serde_json::Error> {
    DialRequest::deserialize(v)
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

#[cfg(test)]
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
