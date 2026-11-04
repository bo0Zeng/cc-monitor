//! SSH 的一切都在这里，而且只在本机那一个常驻后端里。
//!
//! 本机只常驻一个后端；到各远端的 SSH 连接由它持有、按拨号身份复用（拨号核心 `ssh_dial::pool`）。界面不开到远端的字节流：
//! 问那台一律交本机后端（各台的常驻流 `control/feeds.rs` · 问远端后端 [`remote_ask`] · 传输 · 部署 · 开终端），本后端照机器表按名字组拨号请求（[`DialRequest`]）。
//! 多地址竞速 · 跳板 · host key 校验（严格 / TOFU，回报实得指纹）· 鉴权（`key_path` 私钥文件 · ssh-agent）· keepalive ·
//! 开 channel 之后的几种用法（[`Use`]）住拨号核心那个 crate（`ssh-dial/`，手机同一份）；本目录是后端这一侧：
//! 装上本后端的那一套（[`host`]：钥匙文件 · cc-monitor 那份 known_hosts · 本机 agent · 详情事实 · 部署那一处写根）、
//! 端口转发的账 · 测试连接 · 问远端后端 · 开终端那一串 · `ssh -G`。
//! 界面那一侧没有 `russh` 拨号、也没有回落：常驻后端不在 ⇒ monitor 报（`inbound_client::local_backend_accepting`）。
//!
//! SFTP 也在拨号核心（`ssh_dial::sftp`）：在池里那条连接上开 sftp 子系统；本后端装的规矩是远端写只许暂存区一处（`~/.cc-monitor/staging/`）。
//! 界面拿不到原始 SFTP 字节（`use:"subsystem"` 回 `unsupported_use`）：装 · 卸走帧命令 `deploy-install` · `deploy-uninstall`，
//! 传输走入方向命令 `transfer-*`（`control/transfer.rs`）。边界见 `INVARIANTS §41.6` 与 `readonly_guard::remote_write_layer`。
//!
//! # 边界
//!
//! - 竞速不错开起拨：本 crate 不许有「睡到点自己醒」的构件（`no_timer_guard`），RFC 8305 那 250 ms 的阶梯做不了 ⇒
//!   所有地址同时起拨，首个握手成功者胜、其余立即丢弃；界面给的顺序（last-good 排首）只决定同时完成时谁先被看到。
//! - ssh-agent 的 Windows 那一半（OpenSSH 命名管道）只在交叉编译上编得过，零真机读数。
//!
//! 独立的包 `backend-dial`（只引 `wire` · `common` · `platform`）：引了上层就编不过。上层只够得到这里的 `pub` 项。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::dial::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as dial;
use backend_common as common;
use backend_platform as platform;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

use std::sync::Once;

pub use ssh_dial::machine;
pub use ssh_dial::machine::Host;
#[cfg(test)]
pub(crate) use ssh_dial::pool;
pub(crate) use ssh_dial::{CaptureOpts, Endpoint, JumpHop, Stage, StageSink, Use};
pub use ssh_dial::{Captured, DialRequest};

pub mod forwards; // 端口转发的账（起 · 停 · 列三条帧命令）
pub mod known_hosts; // cc-monitor 自己那份 known_hosts（握手认下的钥匙；开终端那一行交给 ssh）
pub mod links; // 每一跳可观测：池里的连接读数归到那一台（机器状态成品的 `links`）
pub mod opened_windows; // ↗ 的本机记录：开终端那一刻那个 shell 写下的窗口标签
pub use known_hosts::install as install_known_hosts;
pub mod probe; // 测试连接（`remote-probe`）
               // 本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 那几台（照机器表按名字够到）—— 全后端只此一处。
               // 住拨号层：它只用拨号与契约那几样，不碰帧面（放在 `stream/` 会让 `dial ⇄ stream` 双向都多出几条边）。
pub mod remote_ask;
pub mod ssh_config;
pub mod terminal; // 开终端那一串（`ssh -t …` 外壳 ＋ PowerShell 窗口载荷）在这里渲
pub mod terminal_processes; // ↗ 那一问的本机那一半：终端连接是这台电脑上哪个进程开的、它往上的进程链

/// 开出 channel 之后的几种用法：拨号核心那一份（判据经这里够到；生产那一处是端口转发，进门前先装上本后端的那一套 [`host`]）。
#[cfg(test)]
pub(crate) mod uses {
    pub(crate) use ssh_dial::uses::*;
}

/// SFTP 客户端：拨号核心那一份，传输那一趟进门前先装上本后端的那一套（[`host`]）。
pub mod sftp {
    pub use ssh_dial::sftp::*;

    /// 传输那一趟的会话（[`ssh_dial::sftp::open_for_transfer`]）；先装上本后端的那一套。
    pub async fn open_for_transfer(d: &Dial) -> Result<Session, crate::common::said::Said> {
        super::host();
        ssh_dial::sftp::open_for_transfer(d).await
    }

    /// 台架：起会话之前先装上本后端的那一套（[`super::sftp_rig`]；上层的传输判据经 `test-support` 够到）。
    #[cfg(feature = "test-support")]
    pub use super::sftp_rig as rig;
}

/// 台架（合成 SFTP 服务端）：起会话之前先装上本后端的那一套（写按部署那一处）。
#[cfg(feature = "test-support")]
#[path = "../../../tests/backend/dial_sftp_rig_host.rs"]
pub mod sftp_rig;

/// 装上本后端交给拨号核心的那一套（一个进程一次；进程入口 `stream/entry.rs` 起来就装，进拨号核心的几个口也各装一次兜住测试档）：
/// 钥匙是私钥文件的路径（`~` 按这台家目录展开）· 认下的 host key 记进 cc-monitor 那份 known_hosts · 本机 ssh-agent · 详情写本后端的时刻与机器 ·
/// 远端写只许暂存区那一处（建出来的目录收成 [`crate::common::own_dir::PRIVATE_DIR_MODE`]）。
pub fn host() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let env = ssh_dial::env::Env {
            keys: Box::new(HomeKeyFiles {
                home: crate::platform::paths::home_dir(),
            }),
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

/// 钥匙是私钥文件的路径：打头的 `~` 按这台家目录展开（[`crate::platform::paths::expand_tilde_from`]；
/// 拨号核心不碰家目录也不读盘，展开 · 读都是宿主的事），带上限读成文本，再交拨号核心解（[`ssh_dial::env::key_from_pem`]）；
/// 读不出来那一句里的路径照用户写的。
pub(crate) struct HomeKeyFiles {
    /// 这台的家目录（装的那一下定；说不出 ⇒ `None`，`~` 原样交下去）。
    pub(crate) home: Option<std::path::PathBuf>,
}

/// 钥匙文件的读上限：OpenSSH 私钥几 KiB，超了就不是一把钥匙（拒收，回读不出那一句）。
const KEY_FILE_CAP: relay_route_core::Cap = relay_route_core::Cap {
    bytes: 64 * 1024,
    over: relay_route_core::Over::Reject,
};

impl ssh_dial::env::KeySource for HomeKeyFiles {
    fn credential(&self, key: &str) -> Result<ssh_dial::env::Credential, ssh_dial::Said> {
        let at = crate::platform::paths::expand_tilde_from(key, self.home.as_deref());
        let pem = crate::platform::fs::Ro::read_string_capped(&at, KEY_FILE_CAP).map_err(|e| {
            let why = match e.raw {
                Some(raw) => format!("{} · {raw}", e.said),
                None => e.said,
            };
            Said::with_raw(ssh_dial::env::key_unreadable(key), why)
        })?;
        ssh_dial::env::key_from_pem(key, &pem, None)
    }
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

use crate::common::said::Said;

// ───────────── 这一端的机器表：拨号层只读，读法由 faces 装 ─────────────

/// 这一端的机器表怎么读（`control::machines` 那一份）：起来那一下装一次（拨号层不认控制面：拨号那个包不依赖控制面）。
static MACHINES: std::sync::OnceLock<fn() -> Vec<Host>> = std::sync::OnceLock::new();

/// 装上「这一端的机器表怎么读」。装过再装 ⇒ 不换（一个进程一张表），记一行。
pub fn install_machines(read: fn() -> Vec<Host>) {
    if MACHINES.set(read).is_err() {
        tracing::warn!("dial: 机器表的读法已经装过了，这一次不换");
    }
}

/// 这一端的机器表此刻那一张（现读；没装 ⇒ 空表）。
pub fn machines_now() -> Vec<Host> {
    MACHINES.get().map_or_else(Vec::new, |read| read())
}

/// 那几台此刻有没有一条拨通了的长流（本机后端替那台开的常驻流，接上之后到断掉）：
/// 「那台后端此刻在」只按它判（推公钥先问那台后端还是走一次 exec，`assets/pubkey.rs`）。记的那一处是 `control/feeds.rs`。
pub mod streams {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static UP: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

    fn lock() -> std::sync::MutexGuard<'static, Option<HashMap<String, usize>>> {
        UP.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 那一台此刻有没有拨通了的长流。
    pub fn up(name: &str) -> bool {
        lock()
            .as_ref()
            .and_then(|m| m.get(name))
            .is_some_and(|n| *n > 0)
    }

    /// 一条拨通了的长流：活着算一条，被丢就减掉。
    pub struct Held(String);

    /// 记一条拨通了的长流。
    pub fn hold(name: &str) -> Held {
        *lock()
            .get_or_insert_with(HashMap::new)
            .entry(name.to_string())
            .or_insert(0) += 1;
        Held(name.to_string())
    }

    impl Drop for Held {
        fn drop(&mut self) {
            let mut g = lock();
            if let Some(m) = g.as_mut() {
                if let Some(n) = m.get_mut(&self.0) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        m.remove(&self.0);
                    }
                }
            }
        }
    }
}

/// 表里叫 `name` 的那一台 ⇒ 一份组好的拨号请求（蛇形键）：组法 · 判准 · 跳板一路只在 [`machine::request`]，照表里的次序竞速。
/// ssh-agent 套接字不交 ⇒ 拨号核心用本进程自己的（`DialRequest::agent_sock` 缺席那一格）。
pub fn request_named(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, Said> {
    machine::request(hosts, name, None, use_, extra)
}

/// 同 [`request_named`]，读成拨号请求。
pub fn dial_named(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<DialRequest, Said> {
    read_request(&request_named(hosts, name, use_, extra)?)
}

/// 同 [`request_named`]，两样都交：组好的那一份（问那台 · 存着再用）与读成的拨号请求（开会话用）。
pub fn named_both(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<(serde_json::Value, DialRequest), Said> {
    let v = request_named(hosts, name, use_, extra)?;
    let req = read_request(&v)?;
    Ok((v, req))
}

/// 传输那一趟的拨号（[`sftp::Dial`]）：按名字照表组（同 [`named_both`]）。
pub fn sftp_named(hosts: &[Host], name: &str) -> Result<(serde_json::Value, sftp::Dial), String> {
    let (v, req) = named_both(hosts, name, "capture", serde_json::json!({})).map_err(|f| f.said)?;
    Ok((v, sftp::Dial::of(req)))
}

/// 一份组好的拨号请求（[`request_named`] 那一形）⇒ 拨号请求。
pub(crate) fn read_request(v: &serde_json::Value) -> Result<DialRequest, Said> {
    <DialRequest as serde::Deserialize>::deserialize(v).map_err(|e| {
        Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!("dial request unreadable: {e}")),
        )
    })
}

/// 还没存进表的那一台（测试连接 · 推公钥测的那一台）⇒ 组好的拨号请求：它顶替表里同名的那一台（没有就加上），跳板照表按名字找。
pub fn request_draft(
    hosts: &[Host],
    draft: &Host,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, Said> {
    let mut table: Vec<Host> = hosts
        .iter()
        .filter(|h| h.name() != draft.name())
        .cloned()
        .collect();
    table.push(draft.clone());
    request_named(&table, draft.name(), use_, extra)
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
