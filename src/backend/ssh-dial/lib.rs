//! **拨号核心**：到一台远端的 SSH 的全部活 —— 后端（本机常驻那一个）与手机（`src/mobile/core-rs`）同一份。
//!
//! 多地址竞速 · 跳板（可多跳）· host key 校验（按一份库，[`known_hosts`]）· 鉴权（钥匙 · ssh-agent · 外部签名器 · 密码）·
//! keepalive · 按拨号身份复用（[`pool`]）· 开 channel 之后的几种用法（[`Use`]，含 PTY）· SFTP 客户端（[`sftp`]，写的规矩是参数）。
//!
//! 平台的事从 [`env`] 的几个口进：钥匙从哪来（[`env::KeySource`]）· 主机钥匙那份库（[`env::HostKeys`]）·
//! ssh-agent（[`env::Agent`]，只桌面有）· 复制详情那几行事实（[`env::Facts`]）。本 crate 不碰家目录、不读环境变量。
//!
//! # 一条链路上的字节（[`uses::run`]）
//!
//! ```text
//! 上行  stream / shell 时是原样写进 SSH channel 的字节；capture · forward 时不读（读到 EOF ＝ 对面走了）
//! 下行  ack 之前只有一种行 {"stage":{…}}（[`StageLine`]，契约 `cmd_wire::dial`）：stages=true 时攒下的拨号阶段在 ack 之前一次写出；
//!       progress=true 时每过一步当场一行 {"stage":{"kind":"step","step":"tcp|banner|hostkey|auth|tunnel|channel"}}（[`step`]；不攒）
//!       然后恰好一行 ack（DialAck，`\n` 结尾）
//!       其后按用法：stream / shell 原样字节 · capture 一行 Captured 后结束 ·
//!       forward 每接进一条连接一行 {"accepted":n}
//! ```
//!
//! ack 把「断开」与「连上了但远端还没说话」分开；ack 带 `v` 与 `uses`，对面据 `uses` 当场认出这一份不认哪种用法。
//!
//! # 边界
//!
//! - 竞速不错开起拨：本 crate 不许有「睡到点自己醒」的构件（后端那条 `no_timer_guard` 管到这里），RFC 8305 那 250 ms 的阶梯做不了 ⇒
//!   所有地址同时起拨，首个握手成功者胜、其余立即丢弃；给的顺序（last-good 排首）只决定同时完成时谁先被看到。
//! - 平台 cfg：android 不算 linux。平台分叉各臂穷举、不写「其余平台」那一臂，android 那一臂明写：漏了就编不过
//!   （门禁 `androidbuild` · `mobile` 两格编手机两个 target）。

// 测试段可以用会自己醒来的构件（夹具的期限）；生产段的放行逐处写在调用点（`../clippy.toml`）。
#![cfg_attr(test, allow(clippy::disallowed_methods))]
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::io::AsyncWriteExt;

pub use copy_core::said::Said;

pub mod config_text;
pub mod connect;
pub mod env;
pub mod known_hosts;
pub mod machine;
pub mod plain_path;
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

/// 线上那几样（ack 的协议版本与用法表 · 地址 · 阶段 · ack · 没开成 · 收全的结果）住契约（`cmd_wire::dial`），读它的那一侧同一份。
/// 拨号请求那几样（[`DialRequest`] · [`JumpHop`] · [`Use`] · [`PtyOpts`] · [`CaptureOpts`] · [`ForwardSpec`]）也住那里：界面 · 手机交的就是那一份。
pub use cmd_wire::dial::{
    CaptureOpts, Captured, DialAck, DialFail, DialRequest, Endpoint, ForwardSpec, JumpHop, PtyOpts,
    Stage, StageLine, Use, ACK_V, USES,
};

/// 没开成：那一句 ＋ 下层原话（进 `detail`，按宿主的 [`env::Facts`] 写成复制详情）＋ 看到的指纹。
pub fn fail(error: Said, fingerprint: Option<String>) -> DialFail {
    DialFail {
        detail: Some(env::get().facts.detail("dial", None, error.raw.as_deref())),
        error: error.said,
        fingerprint,
        reason: None,
        key_type: None,
        expected_fingerprint: None,
    }
}

/// 带上这一趟记下的原因码与握手时看到的 host key 类型 ·（钥匙被拒时）库里那一把的指纹（[`StageSink`]）。
pub fn noted(f: DialFail, stages: &StageSink) -> DialFail {
    DialFail {
        reason: stages.why(),
        key_type: stages.key_type(),
        expected_fingerprint: stages.expected(),
        ..f
    }
}

/// 没拨成的原因码（ack 的 `reason`；线上形状见协议文档 `link-open` 一节）。
pub use cmd_wire::machine::Why;

/// 竞速那一格的阶段标签（[`connect::stage_of_io`] 那几个，外加 `resolve`）→ 原因码。
pub fn why_of_stage(stage: &str) -> Why {
    match stage {
        "resolve" => Why::Resolve,
        "tcp" => Why::Unreachable,
        "timeout" => Why::Timeout,
        "hostkey" => Why::HostKey,
        _ => Why::Other,
    }
}

/// 拨号过了哪一步（[`StageLine::Step`] 的 `step`）：每过一步报一次，经跳板时跳板那几台与目标那一台各走一遍。
/// 只报「过了」：没过的那一步由 ack 说。读它的那一侧只拿它重新计时，不按名字分支。
pub mod step {
    /// 那个口的 TCP 连上了。
    pub const TCP: &str = "tcp";
    /// 对面的第一批字节到了（SSH 头）。
    pub const BANNER: &str = "banner";
    /// 密钥交换做完、host key 校验过了。
    pub const HOSTKEY: &str = "hostkey";
    /// 认证过了。
    pub const AUTH: &str = "auth";
    /// 经跳板开出了到下一台的隧道。
    pub const TUNNEL: &str = "tunnel";
    /// 通道开了（exec · sftp · shell 那一条；之后就是 ack）。
    pub const CHANNEL: &str = "channel";
}

/// 阶段行的出口。`stages=false` 时一个字节都不写。
///
/// 阶段是从握手的好几个并发任务里产生的，而 stdout 只有一个 ⇒ 先进一个共享的缓冲，
/// ack 之前按到达顺序一次写出去（阶段只在 ack 之前有意义）。
#[derive(Clone, Default)]
pub struct StageSink {
    on: bool,
    buf: Arc<Mutex<Vec<Stage>>>,
    /// 这一趟没拨成的原因码（不论要不要阶段行都记）。
    why: Arc<Mutex<Option<Why>>>,
    /// 这一趟最后一次握手时服务端给的 host key 类型。
    key_type: Arc<Mutex<Option<String>>>,
    /// 钥匙被拒时库里那一把的指纹（[`DialAck::expected_fingerprint`]）。
    expected: Arc<Mutex<Option<String>>>,
    /// 进展往哪报（请求带 `progress` 时 [`uses::run_with`] 接上；没有 ⇒ 不报）。
    steps: Option<tokio::sync::mpsc::UnboundedSender<&'static str>>,
}

impl StageSink {
    pub fn new(on: bool) -> Self {
        StageSink {
            on,
            buf: Arc::default(),
            why: Arc::default(),
            key_type: Arc::default(),
            expected: Arc::default(),
            steps: None,
        }
    }

    /// 同一份，另把进展报到回的那一头（[`step`] 那几个）。
    pub fn reporting(
        &self,
    ) -> (
        StageSink,
        tokio::sync::mpsc::UnboundedReceiver<&'static str>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            StageSink {
                steps: Some(tx),
                ..self.clone()
            },
            rx,
        )
    }

    /// 不记阶段 · 原因码 · host key 类型的一份，进展照报到同一头（赛跑里另拨的那一条：它多半被丢掉，但这一趟在等它）。
    pub fn quiet(&self) -> StageSink {
        StageSink {
            steps: self.steps.clone(),
            ..StageSink::new(false)
        }
    }

    /// 过了一步（[`step`]）。
    pub fn step(&self, s: &'static str) {
        if let Some(tx) = &self.steps {
            let _ = tx.send(s);
        }
    }

    /// 记下没拨成的原因码（后记的盖前记的：跳板那一层比它里面那次竞速更说明问题）。
    pub fn note_why(&self, w: Why) {
        *self.why.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
    }

    /// 记下握手时服务端给的 host key 类型。
    pub fn note_key_type(&self, t: &str) {
        *self.key_type.lock().unwrap_or_else(|e| e.into_inner()) = Some(t.to_string());
    }

    /// 记下钥匙被拒时库里那一把的指纹（空串 = 库里没有同类型的）。
    pub fn note_expected(&self, fp: &str) {
        *self.expected.lock().unwrap_or_else(|e| e.into_inner()) = Some(fp.to_string());
    }

    pub fn expected(&self) -> Option<String> {
        self.expected
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn key_type(&self) -> Option<String> {
        self.key_type
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn why(&self) -> Option<Why> {
        *self.why.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn emit(&self, s: Stage) {
        if self.on {
            self.buf.lock().unwrap_or_else(|e| e.into_inner()).push(s);
        }
    }

    /// 攒下的阶段照到达顺序取走（写阶段行那一处 · 进程内要阶段的调用方：测试连接）。
    pub fn drain(&self) -> Vec<Stage> {
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
