//! SSH 的一切都在这里，而且只在本机那一个常驻后端里。
//!
//! 本机只常驻一个后端；到各远端的 SSH 连接由它持有、按拨号身份复用（拨号核心 `ssh_dial::pool`）。monitor 要跟某台远端说话时，
//! 经它与本机后端之间那条已有的流开一条链路（`link-open`，[`link`]），交一份拨号请求（[`DialRequest`]），然后只在这条链路上收发字节。
//! 多地址竞速 · 跳板 · host key 校验（严格 / TOFU，回报实得指纹）· 鉴权（`key_path` 私钥文件 · ssh-agent）· keepalive ·
//! 开 channel 之后的几种用法（[`Use`]）住拨号核心那个 crate（`ssh-dial/`，手机同一份）；本目录是后端这一侧：
//! 装上本后端的那一套（[`host`]：钥匙文件 · cc-monitor 那份 known_hosts · 本机 agent · 详情事实 · 部署那一处写根）、
//! 帧面链路（[`link`]）· 端口转发的账 · 测试连接 · 问远端后端 · 开终端那一串 · `ssh -G`。
//! 界面那一侧没有 `russh` 拨号、也没有回落：常驻后端不在 ⇒ monitor 报（`dial_host.rs`）。
//!
//! SFTP 也在拨号核心（`ssh_dial::sftp`）：在池里那条连接上开 sftp 子系统；本后端装的规矩是远端写只许暂存区一处（`~/.cc-monitor/staging/`）。
//! 界面拿不到原始 SFTP 字节（`use:"subsystem"` 回 `unsupported_use`）：装 · 卸走帧命令 `deploy-install` · `deploy-uninstall`，
//! 传输走入方向命令 `transfer-*`（`control/transfer.rs`）；链路 `use:"files"`（受限的远端文件一问一答）协议上还在、界面已不用。边界见 `INVARIANTS §41.6` 与 `readonly_guard::remote_write_layer`。
//!
//! # 一条链路上的字节（线上帧见 `wire.rs` 的 `LinkData`）
//!
//! ```text
//! 界面 → 后端  link-open：{"link","window","dial":DialRequest}
//! link-data：stream 时是原样写进 SSH channel 的字节；capture 时不读；forward 时不读
//! link-close：界面走了，收工
//! 后端 → 界面  stages=true 时先有若干行 {"stage":{…}}（与界面 `ConnectStage` 同形）；progress=true 时每过一步当场一行 {"progress":…}（`ssh_dial::step`）
//! 然后恰好一行 ack（DialAck，`\n` 结尾）
//! 其后按用法：stream 原样字节 · capture 一行 Captured 后结束 ·
//! forward 每接进一条连接一行 {"accepted":n} ·
//! files 之后一问一答（上行一行请求、下行一行应答，`sftp.rs` 头注）
//! ```
//!
//! ack 把「断开」与「连上了但远端还没说话」分开；ack 带 `v` 与 `uses`，界面据 `uses` 当场认出这个后端不认哪种用法。
//!
//! # 边界
//!
//! - 竞速不错开起拨：本 crate 不许有「睡到点自己醒」的构件（`no_timer_guard`），RFC 8305 那 250 ms 的阶梯做不了 ⇒
//!   所有地址同时起拨，首个握手成功者胜、其余立即丢弃；界面给的顺序（last-good 排首）只决定同时完成时谁先被看到。
//! - ssh-agent 的 Windows 那一半（OpenSSH 命名管道）只在交叉编译上编得过，零真机读数。

use std::sync::Once;

pub(crate) use ssh_dial::machine;
#[cfg(test)]
pub(crate) use ssh_dial::pool;
pub use ssh_dial::{
    why_of_stage, CaptureOpts, Captured, DialAck, DialRequest, Endpoint, ForwardSpec, JumpHop,
    Stage, StageSink, Use, Why, ACK_V, USES,
};

pub(crate) mod forwards; // 端口转发的账（起 · 停 · 列三条帧命令）
pub(crate) mod known_hosts; // cc-monitor 自己那份 known_hosts（握手认下的钥匙；开终端那一行交给 ssh）
pub use known_hosts::install as install_known_hosts;
pub mod link;
// 本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 可达表 —— 全后端只此一处；帧面 `remote-reach`。
// 住拨号层：它只用拨号与契约那几样，不碰帧面（审计二 B2e：原住 `stream/`，那时 `dial ⇄ stream` 双向都因它多出几条边）。
pub(crate) mod pick; // 帧命令入参里「拨哪台」那几格的类型（组请求在 `ssh_dial::machine`）
pub(crate) mod probe; // 测试连接（`remote-probe`）
pub mod remote_ask;
pub(crate) mod ssh_config;
pub(crate) mod terminal; // 开终端那一串（`ssh -t …` 外壳 ＋ PowerShell 窗口载荷）在这里渲
pub(crate) mod terminal_processes; // ↗ 那一问：那台报来的终端连接是这台电脑上哪个进程开的、它往上的进程链

/// 开出 channel 之后的几种用法：拨号核心那一份，进门前先装上本后端的那一套（[`host`]）。
pub(crate) mod uses {
    #[cfg(test)]
    pub(crate) use ssh_dial::uses::*;

    /// 一份拨号请求的完整一趟（[`ssh_dial::uses::run`]）；先装上本后端的那一套。
    pub(crate) fn run<'a, R, W>(
        req: &'a super::DialRequest,
        stages: &'a super::StageSink,
        input: R,
        out: &'a mut W,
    ) -> impl std::future::Future<Output = ()> + Send + 'a
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'a,
        W: tokio::io::AsyncWrite + Unpin + Send,
    {
        super::host();
        ssh_dial::uses::run(req, stages, input, out)
    }
}

/// SFTP 客户端：拨号核心那一份，传输那一趟进门前先装上本后端的那一套（[`host`]）。
pub(crate) mod sftp {
    pub(crate) use ssh_dial::sftp::*;

    /// 传输那一趟的会话（[`ssh_dial::sftp::open_for_transfer`]）；先装上本后端的那一套。
    pub(crate) async fn open_for_transfer(d: &Dial) -> Result<Session, crate::common::said::Said> {
        super::host();
        ssh_dial::sftp::open_for_transfer(d).await
    }

    /// 台架：起会话之前先装上本后端的那一套（[`super::sftp_rig`]）。
    #[cfg(test)]
    pub(crate) use super::sftp_rig as rig;
}

/// 台架（合成 SFTP 服务端）：起会话之前先装上本后端的那一套（写按部署那一处）。
#[cfg(test)]
#[path = "../../../tests/backend/dial_sftp_rig_host.rs"]
pub(crate) mod sftp_rig;

/// 装上本后端交给拨号核心的那一套（一个进程一次；`main` 起来就装，进拨号核心的几个口也各装一次兜住测试档）：
/// 钥匙是私钥文件的路径 · 认下的 host key 记进 cc-monitor 那份 known_hosts · 本机 ssh-agent · 详情写本后端的时刻与机器 ·
/// 远端写只许暂存区那一处（建出来的目录收成 [`crate::common::own_dir::PRIVATE_DIR_MODE`]）。
pub fn host() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let env = ssh_dial::env::Env {
            keys: Box::new(ssh_dial::env::KeyFiles),
            host_keys: Box::new(known_hosts::KnownHostsFile),
            agent: Some(Box::new(crate::platform::ssh_agent::LocalAgent)),
            facts: Box::new(BackendFacts),
            files: ssh_dial::sftp::WriteRules::Deploy {
                dir_mode: crate::common::own_dir::PRIVATE_DIR_MODE,
            },
        };
        if ssh_dial::env::install(env).is_err() {
            tracing::error!(
                "dial: 拨号核心那一套已经被别人装过了 —— 本后端的钥匙 / known_hosts / agent 不在用"
            );
        }
    });
}

/// 复制详情那几行：本后端的时刻与机器（`wire/detail.rs`）· 本后端的钟（`common/time.rs`）。
struct BackendFacts;

impl ssh_dial::env::Facts for BackendFacts {
    fn detail(&self, cmd: &str, code: Option<&str>, raw: Option<&str>) -> String {
        match code {
            Some(code) => crate::wire::detail::of(Some(cmd), code, raw),
            None => crate::wire::detail::of_run(cmd, raw),
        }
    }

    fn now(&self) -> std::time::SystemTime {
        crate::common::time::now()
    }
}

/// 解析 `link-open` 的 `dial` 字段；抽出来让判据不开真链路就验得了「蛇形键读得动」。
/// 线上交来的那份拨号（一台机器原样的配置）⇒ 拨号请求 —— 组法只在 [`machine::resolve`]。
pub(crate) fn parse_request_value(
    v: &serde_json::Value,
) -> Result<DialRequest, (&'static str, String)> {
    machine::resolve(v)
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
