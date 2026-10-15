//! **拨号核心**：到一台远端的 SSH 的全部活 —— 后端（本机常驻那一个）与手机（`src/mobile/core-rs`）同一份。
//!
//! 多地址竞速 · 跳板（可多跳）· host key 校验（严格 / TOFU，回报实得指纹）· 鉴权（钥匙 · ssh-agent · 外部签名器 · 密码）·
//! keepalive · 按拨号身份复用（[`pool`]）· 开 channel 之后的几种用法（[`Use`]，含 PTY）· SFTP 客户端（[`sftp`]，写的规矩是参数）。
//!
//! 平台的事从 [`env`] 的几个口进：钥匙从哪来（[`env::KeySource`]）· 认下的 host key 记到哪（[`env::HostKeys`]）·
//! ssh-agent（[`env::Agent`]，只桌面有）· 复制详情那几行事实（[`env::Facts`]）。本 crate 不碰家目录、不读环境变量。
//!
//! # 一条链路上的字节（[`uses::run`]）
//!
//! ```text
//! 上行  stream / shell 时是原样写进 SSH channel 的字节；capture · forward 时不读（读到 EOF ＝ 对面走了）
//! 下行  stages=true 时先有若干行 {"stage":{…}}（与界面 `ConnectStage` 同形）
//!       然后恰好一行 ack（DialAck，`\n` 结尾）
//!       其后按用法：stream / shell 原样字节 · capture 一行 Captured 后结束 ·
//!       forward 每接进一条连接一行 {"accepted":n} ·
//!       files 之后一问一答（上行一行请求、下行一行应答，`sftp.rs` 头注）
//! ```
//!
//! ack 把「断开」与「连上了但远端还没说话」分开；ack 带 `v` 与 `uses`，对面据 `uses` 当场认出这一份不认哪种用法。
//!
//! # 边界
//!
//! - 竞速不错开起拨：本 crate 不许有「睡到点自己醒」的构件（后端那条 `no_timer_guard` 管到这里），RFC 8305 那 250 ms 的阶梯做不了 ⇒
//!   所有地址同时起拨，首个握手成功者胜、其余立即丢弃；给的顺序（last-good 排首）只决定同时完成时谁先被看到。
//! - 平台 cfg：android 不算 linux，凡是点名 linux 的 cfg 都要把 android 明写出来（`android_cfg_guard`）。

// 测试段可以用会自己醒来的构件（夹具的期限）；生产段的放行逐处写在调用点（`../clippy.toml`）。
#![cfg_attr(test, allow(clippy::disallowed_methods))]
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

pub use copy_core::said::Said;

pub mod b64;
pub mod config_text;
pub mod connect;
pub mod env;
pub mod known_hosts;
pub mod machine;
pub mod pool;
pub mod sftp;
pub mod tcp_rtt;
pub mod uses;

/// 拨了必被拒、又不会被别人捡走的本机口（测试用）：本 crate 的判据与后端的测试档（经 `test-support`）共用这一份。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/refusing_port.rs"]
pub mod refusing_port;

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_support.rs"]
mod guard_support;

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_features_tests.rs"]
mod features_tests;

#[cfg(test)]
#[path = "../../../tests/backend/ssh_dial_android_cfg_guard.rs"]
mod android_cfg_guard;

/// 契约错的那一句（`detail`：英文诊断，不进表、不上句子，记一行日志）。后端 `common/contract.rs` 转用这一份（全仓只此一处）。
/// 诊断跟着这一串走（隔一个 [`DIAG_SEP`]），由失败信封那一处（[`parts`]）拆开：句子照旧只有「请求格式不对」，
/// 诊断进复制详情的「原话」那一行。
pub fn malformed(detail: &str) -> String {
    tracing::warn!("contract: malformed request: {detail}");
    format!(
        "{}{DIAG_SEP}{detail}",
        copy_core::copy_text("beContract.malformed.say", &[])
    )
}

/// 句子与跟在它后面的契约诊断之间那一个字符（[`malformed`]）。
const DIAG_SEP: char = '\u{1f}';

/// 一条失败的那一句 ⇒ (给人看的那一句, 下层原话)：句子后面跟着契约诊断 ⇒ 拆出来、放在原话最前面；没有诊断 ⇒ 原样。
pub fn parts(message: &str, raw: Option<&str>) -> (String, Option<String>) {
    match message.split_once(DIAG_SEP) {
        Some((said, diag)) => (
            said.to_string(),
            Some(match raw {
                Some(r) => format!("{diag}\n{r}"),
                None => diag.to_string(),
            }),
        ),
        None => (message.to_string(), raw.map(str::to_string)),
    }
}

/// [`parts`] 的反方向：那一句 ＋ 原话 ⇒ 一串（原话隔一个 [`DIAG_SEP`] 跟在后面；没有原话 ⇒ 那一句原样）。
pub fn joined(said: String, raw: Option<String>) -> String {
    match raw {
        Some(r) => format!("{said}{DIAG_SEP}{r}"),
        None => said,
    }
}

/// ack 里的协议版本：v2 = 竞速 · 跳板 · agent · 几种用法 · 阶段行。多一种用法不 bump 它（新界面凭 `uses` 认出老后端），
/// 版本号只在同一种用法的字节形状变了时才动。
pub const ACK_V: u32 = 2;

/// 本代理认得的用法 —— ack 的 `uses` 字段原样回这张表，界面据它判「代理够不够新」。
pub const USES: &[&str] = &["stream", "capture", "forward", "files", "shell", "hold"];

/// 一个拨号地址。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

/// 跳板那一台。它自己还要经别的跳板 ⇒ `jump` 再套一层（多跳，同 `ssh -J 外,内`；桌面的机器表今天只给一跳，手机给多跳）。
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
    /// 这一台自己经哪台跳板到（再外一台；`None` = 本机直连它）。
    #[serde(default)]
    pub jump: Option<Box<JumpHop>>,
    /// 这一台记过的 host key（同 [`DialRequest::known_keys`]）。
    #[serde(default)]
    pub known_keys: Vec<String>,
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
    /// 本进程绑本机回环口、每接进一条连接开一条 direct-tcpip（端口转发）。
    Forward,
    /// 开 sftp 子系统，之后在链路上一问一答（远端文件操作：写的规矩由宿主定，[`sftp::WriteRules`]）。
    Files,
    /// 要一个 PTY（[`PtyOpts`]）开交互 shell，之后同 `stream` 原样对拷；改窗口大小走 [`uses::Control`]。
    Shell,
    /// 只握着这条连接（不开通道）：ack 之后链路不收工，同一身份的别的链路都复用它；对面走了（上行 EOF）
    /// 或连接没了（russh 报断开）⇒ 收工。手机那一形：「连上一台机器」就是开一条这个。
    Hold,
}

/// `use: shell` 的 PTY。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PtyOpts {
    #[serde(default = "PtyOpts::default_term")]
    pub term: String,
    pub cols: u32,
    pub rows: u32,
}

impl PtyOpts {
    fn default_term() -> String {
        "xterm-256color".to_string()
    }
}

impl Default for PtyOpts {
    fn default() -> Self {
        PtyOpts {
            term: Self::default_term(),
            cols: 80,
            rows: 24,
        }
    }
}

/// `use: capture` 的参数。
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureOpts {
    /// stdout / stderr 各自的收集上限（字节）。
    pub max_bytes: usize,
    /// stdout 一出现这个子串就收工（不认参数的老后端会掉进流模式、永不 EOF）。
    #[serde(default)]
    pub abort_marker: Option<String>,
    /// exec 之后原样写进远端进程 stdin 的字节（缺席 = 一个字节不写）。**不关 stdin** ——
    /// 收它的那一侧是后端 CLI 面的「只读一行」入口（`wire/cli_words.rs::STDIN_LINE_FLAG`），读到换行就动手。
    /// 有了它，载荷不必拼进命令行（那要求远端登录 shell 认 POSIX 引号与管道）。
    #[serde(default)]
    pub stdin: Option<String>,
}

/// `use: forward` 的参数。
#[derive(Debug, Clone, Deserialize)]
pub struct ForwardSpec {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// 界面交给代理的那份 JSON。蛇形键（不是界面配置那套 camelCase）：这条管子两端都是我们自己，不与给前端看的契约拴在一起。
/// v2 的字段全是可选的（老界面发来的请求照样读得动）。
#[derive(Debug, Clone, Deserialize)]
pub struct DialRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 钥匙那一格（不是私钥本体）：桌面是 OpenSSH 格式私钥的**路径**，手机是那台机器的钥匙编号；
    /// 由宿主的 [`env::KeySource`] 换成能鉴权的东西（钥匙 · 外部签名器 · 密码）。缺席 = 走 ssh-agent。
    pub key_path: Option<String>,
    /// 期望的 host key 指纹（`SHA256:…`）。`None` = TOFU 接受并 `warn`。
    pub host_key_fingerprint: Option<String>,
    /// 要 exec 的命令行（`stream` / `capture`；界面已 `shell_quote` 过；`forward` 不用它）。
    #[serde(default)]
    pub command: String,
    /// 竞速的地址顺序（`machine.rs::resolve` 按 `prefer` 排好）。空 = 只有 `host:port`。
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    /// 经这台跳板（fail-closed：它断开就报错，不回落直连）。
    #[serde(default)]
    pub jump: Option<JumpHop>,
    #[serde(default, rename = "use")]
    pub use_: Use,
    #[serde(default)]
    pub capture: Option<CaptureOpts>,
    #[serde(default)]
    pub forward: Option<ForwardSpec>,
    /// `use: shell` 的 PTY（缺 ⇒ [`PtyOpts::default`]）。
    #[serde(default)]
    pub pty: Option<PtyOpts>,
    /// ack 之前逐行报阶段（测试连接那六格）。
    #[serde(default)]
    pub stages: bool,
    /// 这一台记过的 host key（OpenSSH 那一行的前两段：`算法 base64`，可多种类型；手机从 Room 交来）。非空 ⇒ 同 OpenSSH：
    /// 协商时这几种类型排前、按协商出的那一种严格比；那一种没记过 ⇒ 拒（同「钥匙变了」）。空 ⇒ 看 `host_key_fingerprint`。
    #[serde(default)]
    pub known_keys: Vec<String>,
    /// 短命探活（inactivity 拆链）而不是长连接（keepalive 保活）。
    #[serde(default)]
    pub probe: bool,
    /// ssh-agent 套接字的路径（Unix；界面进程此刻的 `SSH_AUTH_SOCK`）。要界面交过来：常驻后端活得比任何一个界面进程都长，
    /// 它自己环境里那份是第一个起它的界面给的，用户重新登录之后 agent 换了路径。缺席 = 用本进程自己的。
    /// 它是一条路径、不是凭据本体；不进连接池的身份（换 agent 不该换连接）。
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
    /// `ok=false` 时的那一句（只带原因词，不带下层原话）。界面把它原样冒泡给重连那一层。
    pub error: Option<String>,
    /// `ok=false` 时的复制详情（时刻 · 机器 · 命令 · 下层原话，后端这一端写好；排法同失败应答那一格）。成功 ⇒ 不出这一格。additive。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 实际观察到的 host key 指纹 —— 界面侧 TOFU 固化要它（失败时也尽量带上）。
    pub fingerprint: Option<String>,
    /// 〔「保住多地址那一格」〕这一趟报过指纹的每条地址 → 指纹。additive（`ACK_V` 不动）；
    /// 界面只在各地址一致时自动固化。失败的 ack 恒空（指纹照旧在 `fingerprint`）。
    pub fingerprints: std::collections::BTreeMap<String, String>,
    /// 〔第二问〕经跳板时**跳板那一台**报过的指纹（地址 → 指纹）：它是另一台机器，界面按它自己那一格固化
    /// （同 `fingerprints` 的判定）。直连 / 失败 ⇒ 空。additive。
    pub jump_fingerprints: std::collections::BTreeMap<String, String>,
    /// 竞速胜出的地址（`host:port`）—— 测试连接展示「你正连着哪条路」。
    pub endpoint: Option<String>,
    /// 同一条胜者，结构化（`{host, port}`）—— 界面记 last-good、下一趟当 `prefer` 交回来、起终端时拼 ssh 命令用它；
    /// 界面因此不必再解析 `host:port`（地址解析只在 `machine.rs`）。失败 ⇒ `None`。additive。
    pub winner: Option<Endpoint>,
    /// 这一趟请求里目标那台**带了**期望指纹（严格校验）—— 界面判「要不要自动固化」用它，不再自己重推一遍指纹继承规则。
    pub strict: bool,
    /// 同上，跳板那一台（直连 ⇒ `false`）。
    pub jump_strict: bool,
    /// 没拨成时的原因码（闭集，[`why`]）：界面按码说那一句、给那颗修法。拨成了 / 说不清 ⇒ `None`。additive。
    pub reason: Option<&'static str>,
    /// 目标那一台握手时给的 host key 是什么类型（「钥匙变了」那一句要说是哪一种）。没握到那一步 ⇒ 不出这一格。additive。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    /// 协议版本（[`ACK_V`]）。
    pub v: u32,
    /// 本代理认得的用法（[`USES`]）。
    pub uses: &'static [&'static str],
}

impl DialAck {
    /// 没拨成 / 没开成：那一句 ＋ 下层原话（进 `detail`，不进 `error`）。
    pub fn failed(error: Said, fingerprint: Option<String>) -> Self {
        DialAck {
            ok: false,
            detail: Some(env::get().facts.detail("dial", None, error.raw.as_deref())),
            error: Some(error.said),
            fingerprint,
            fingerprints: Default::default(),
            jump_fingerprints: Default::default(),
            endpoint: None,
            winner: None,
            strict: false,
            jump_strict: false,
            reason: None,
            key_type: None,
            v: ACK_V,
            uses: USES,
        }
    }

    /// 带上这一趟记下的原因码（[`StageSink::why`]）。
    pub fn because(self, why: Option<&'static str>) -> Self {
        DialAck {
            reason: why,
            ..self
        }
    }

    /// 带上这一趟握手时看到的 host key 类型（[`StageSink::key_type`]）。
    pub fn seen(self, stages: &StageSink) -> Self {
        DialAck {
            key_type: stages.key_type(),
            ..self
        }
    }
}

/// 拨号没成的原因码（ack 的 `reason`，闭集；线上形状见协议文档 `link-open` 一节）。
pub mod why {
    /// 地址解析不出来（DNS）。
    pub const RESOLVE: &str = "resolve";
    /// 那个口拨不通（拒绝 / 主机或网络不可达）。
    pub const UNREACHABLE: &str = "unreachable";
    /// 拨号 / 握手超时。
    pub const TIMEOUT: &str = "timeout";
    /// 主机指纹与记下的不一样。
    pub const HOST_KEY: &str = "host_key";
    /// 密钥被拒（公钥 · agent 都没过）。
    pub const AUTH: &str = "auth";
    /// 那台只收密码（服务端剩下的方法里没有公钥）。
    pub const PASSWORD: &str = "password";
    /// 私钥文件读不出来。
    pub const KEY_UNREADABLE: &str = "key_unreadable";
    /// 跳板那一台断开（拨号 · 鉴权 · 经它开隧道任一步）。
    pub const JUMP: &str = "jump";
    /// 别的（看人话）。
    pub const OTHER: &str = "other";
    /// 全集（判据两向比：后端那边的线上文档与它对拍）。
    pub const ALL: [&str; 9] = [
        RESOLVE,
        UNREACHABLE,
        TIMEOUT,
        HOST_KEY,
        AUTH,
        PASSWORD,
        KEY_UNREADABLE,
        JUMP,
        OTHER,
    ];
}

/// 竞速那一格的阶段标签（[`connect::stage_of_io`] 那几个，外加 `resolve`）→ 原因码。
pub fn why_of_stage(stage: &str) -> &'static str {
    match stage {
        "resolve" => why::RESOLVE,
        "tcp" => why::UNREACHABLE,
        "timeout" => why::TIMEOUT,
        "hostkey" => why::HOST_KEY,
        _ => why::OTHER,
    }
}

/// 一行阶段（`stages=true` 时在 ack 之前出）。**形状与界面 `stream_source::ConnectStage` 逐字段相同** ——
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
        /// 没登上时那一句（`Said::said`，已带原因词；原话随拨号应答那一格走，不在这里）。线上名沿用 `detail`。
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
pub struct StageSink {
    on: bool,
    buf: Arc<Mutex<Vec<Stage>>>,
    /// 这一趟没拨成的原因码（[`why`] 里那几个；不论要不要阶段行都记）。
    why: Arc<Mutex<Option<&'static str>>>,
    /// 这一趟最后一次握手时服务端给的 host key 类型。
    key_type: Arc<Mutex<Option<String>>>,
}

impl StageSink {
    pub fn new(on: bool) -> Self {
        StageSink {
            on,
            buf: Arc::default(),
            why: Arc::default(),
            key_type: Arc::default(),
        }
    }

    /// 记下没拨成的原因码（后记的盖前记的：跳板那一层比它里面那次竞速更说明问题）。
    pub fn note_why(&self, w: &'static str) {
        *self.why.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
    }

    /// 记下握手时服务端给的 host key 类型。
    pub fn note_key_type(&self, t: &str) {
        *self.key_type.lock().unwrap_or_else(|e| e.into_inner()) = Some(t.to_string());
    }

    pub fn key_type(&self) -> Option<String> {
        self.key_type
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn why(&self) -> Option<&'static str> {
        *self.why.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn emit(&self, s: Stage) {
        if self.on {
            self.buf.lock().unwrap_or_else(|e| e.into_inner()).push(s);
        }
    }

    fn drain(&self) -> Vec<Stage> {
        std::mem::take(&mut *self.buf.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// 写一行 JSON 并 flush。**必须 flush** —— 界面在有界读行上等着它。
pub async fn write_line<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    w: &mut W,
    v: &T,
) -> std::io::Result<()> {
    let mut line = serde_json::to_string(v).map_err(std::io::Error::other)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await
}

/// 把一行 ack 写出去并 flush。
pub async fn write_ack<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    ack: &DialAck,
) -> std::io::Result<()> {
    write_line(w, ack).await
}

/// 先写攒下的阶段行，再写 ack。
pub async fn write_stages_then_ack<W: tokio::io::AsyncWrite + Unpin>(
    w: &mut W,
    stages: &StageSink,
    ack: &DialAck,
) -> std::io::Result<()> {
    for s in stages.drain() {
        write_line(w, &serde_json::json!({ "stage": s })).await?;
    }
    write_ack(w, ack).await
}

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
