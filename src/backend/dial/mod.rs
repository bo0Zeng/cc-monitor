//! SSH 的一切都在这里，而且只在本机那一个常驻后端里。
//!
//! 本机只常驻一个后端；到各远端的 SSH 连接由它持有、按拨号身份复用（拨号核心 `ssh_dial::pool`）。monitor 要跟某台远端说话时，
//! 经它与本机后端之间那条已有的流开一条链路（`link-open`，[`link`]），交一份拨号请求（[`DialRequest`]），然后只在这条链路上收发字节。
//! 多地址竞速 · 跳板 · host key 校验（严格 / TOFU，回报实得指纹）· 鉴权（`key_path` 私钥文件 · ssh-agent）· keepalive ·
//! 开 channel 之后的几种用法（[`Use`]）住拨号核心那个 crate（`ssh-dial/`，手机同一份）；本目录是后端这一侧：
//! 装上本后端的那一套（[`host`]：钥匙文件 · cc-monitor 那份 known_hosts · 本机 agent · 详情事实 · 部署那一处写根）、
//! 帧面链路（[`link`]）· 端口转发的账 · 测试连接 · 问远端后端 · 开终端那一串 · `ssh -G`。
//! 界面那一侧没有 `russh` 拨号、也没有回落：常驻后端不在 ⇒ monitor 报（`inbound_client::local_backend_accepting`）。
//!
//! SFTP 也在拨号核心（`ssh_dial::sftp`）：在池里那条连接上开 sftp 子系统；本后端装的规矩是远端写只许暂存区一处（`~/.cc-monitor/staging/`）。
//! 界面拿不到原始 SFTP 字节（`use:"subsystem"` 回 `unsupported_use`）：装 · 卸走帧命令 `deploy-install` · `deploy-uninstall`，
//! 传输走入方向命令 `transfer-*`（`control/transfer.rs`）。边界见 `INVARIANTS §41.6` 与 `readonly_guard::remote_write_layer`。
//!
//! # 一条链路上的字节（线上帧见 `wire.rs` 的 `LinkData`）
//!
//! ```text
//! 界面 → 后端  link-open：{"link","window","dial":DialRequest}
//! link-data：stream 时是原样写进 SSH channel 的字节；capture 时不读；forward 时不读
//! link-close：界面走了，收工
//! 后端 → 界面  ack 之前只有阶段行 {"stage":{…}}（契约 `cmd_wire::dial::StageLine`）：stages=true 时攒下的拨号阶段；progress=true 时每过一步当场一行 {"stage":{"kind":"step",…}}（`ssh_dial::step`）
//! 然后恰好一行 ack（DialAck，`\n` 结尾）
//! 其后按用法：stream 原样字节 · capture 一行 Captured 后结束 ·
//! forward 每接进一条连接一行 {"accepted":n}
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
pub(crate) use ssh_dial::machine::Host;
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
pub(crate) mod probe; // 测试连接（`remote-probe`）
                      // 本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 那几台（照机器表按名字够到）—— 全后端只此一处。
                      // 住拨号层：它只用拨号与契约那几样，不碰帧面（审计二 B2e：原住 `stream/`，那时 `dial ⇄ stream` 双向都因它多出几条边）。
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

/// 钥匙是私钥文件的路径：打头的 `~` 按这台家目录展开再读（[`crate::platform::paths::expand_tilde_from`]；
/// 拨号核心不碰家目录，展开是宿主的事）；读不出来那一句里的路径照用户写的。
pub(crate) struct HomeKeyFiles {
    /// 这台的家目录（装的那一下定；说不出 ⇒ `None`，`~` 原样交下去）。
    pub(crate) home: Option<std::path::PathBuf>,
}

impl ssh_dial::env::KeySource for HomeKeyFiles {
    fn credential(&self, key: &str) -> Result<ssh_dial::env::Credential, ssh_dial::Said> {
        let at = crate::platform::paths::expand_tilde_from(key, self.home.as_deref());
        ssh_dial::env::key_file(key, &at)
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

type CmdErr = (&'static str, String);

// ───────────── 这一端的机器表：拨号层只读，读法由 faces 装 ─────────────

/// 这一端的机器表怎么读（`control::machines` 那一份）：起来那一下装一次（拨号层不认控制面，`layering_guard` 的 dial ↛ control）。
static MACHINES: std::sync::OnceLock<fn() -> Vec<Host>> = std::sync::OnceLock::new();

/// 装上「这一端的机器表怎么读」。装过再装 ⇒ 不换（一个进程一张表），记一行。
pub fn install_machines(read: fn() -> Vec<Host>) {
    if MACHINES.set(read).is_err() {
        tracing::warn!("dial: 机器表的读法已经装过了，这一次不换");
    }
}

/// 这一端的机器表此刻那一张（现读；没装 ⇒ 空表）。
pub(crate) fn machines_now() -> Vec<Host> {
    MACHINES.get().map_or_else(Vec::new, |read| read())
}

/// 「上次连上的那条地址」：按名字记竞速胜者，连同记下那一刻这台的地址（配置改过就失效）。进程内软状态，丢了只是少一次排首。
/// 记的那一处是开链路那一趟读到的 ack（[`link`]）；读的是组请求那一处（[`request_named`]）。
pub(crate) mod last_good {
    use super::{Endpoint, Host};
    use std::collections::HashMap;
    use std::sync::Mutex;

    type Won = HashMap<String, (Vec<Endpoint>, Endpoint)>;

    static WON: Mutex<Option<Won>> = Mutex::new(None);

    fn lock() -> std::sync::MutexGuard<'static, Option<Won>> {
        WON.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 表里叫 `name` 的那一台上次赢的那条（它的地址自那以后没变才算）。
    pub(crate) fn of(hosts: &[Host], name: &str) -> Option<Endpoint> {
        let h = hosts.iter().find(|h| h.name() == name)?;
        let g = lock();
        let (seen, won) = g.as_ref()?.get(name)?;
        (seen == &h.endpoints()).then(|| won.clone())
    }

    /// 记下 `name` 那一台这一趟赢的那条（表里没有这一台 ⇒ 不记）。
    pub(crate) fn record(hosts: &[Host], name: &str, won: &Endpoint) {
        let Some(h) = hosts.iter().find(|h| h.name() == name) else {
            return;
        };
        lock()
            .get_or_insert_with(HashMap::new)
            .insert(name.to_string(), (h.endpoints(), won.clone()));
    }
}

/// 那几台此刻有没有一条拨通了的长流（monitor 连那台后端的那条 `use: stream` 链路，ack 说 `ok` 之后到链路收掉）：
/// 「那台后端此刻在」只按它判（推公钥先问那台后端还是走一次 exec，`assets/pubkey.rs`）。记的那一处是 [`link`]。
pub(crate) mod streams {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static UP: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

    fn lock() -> std::sync::MutexGuard<'static, Option<HashMap<String, usize>>> {
        UP.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 那一台此刻有没有拨通了的长流。
    pub(crate) fn up(name: &str) -> bool {
        lock()
            .as_ref()
            .and_then(|m| m.get(name))
            .is_some_and(|n| *n > 0)
    }

    /// 一条拨通了的长流：活着算一条，被丢就减掉。
    pub(crate) struct Held(String);

    /// 记一条拨通了的长流。
    pub(crate) fn hold(name: &str) -> Held {
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

/// 界面进程最近交来的 ssh-agent 套接字（`link-open` 的 `agent_sock`）：本后端自己去拨的那几趟（问那台 · 传输 · 部署）也用它 ——
/// 常驻后端活得比界面长、起它的那个环境里不一定有 agent。没交过 ⇒ 用本进程自己的。
pub(crate) mod agent_sock {
    use std::sync::Mutex;

    static SEEN: Mutex<Option<String>> = Mutex::new(None);

    pub(crate) fn remember(sock: &str) {
        if !sock.is_empty() {
            *SEEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(sock.to_string());
        }
    }

    pub(crate) fn seen() -> Option<String> {
        SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// 表里叫 `name` 的那一台 ⇒ 一份组好的拨号请求（蛇形键）：组法 · 判准 · 跳板一路只在 [`machine::request`]，上次赢的那条排首；
/// `extra` 没给 `agent_sock` ⇒ 补上界面最近交来的那一个（[`agent_sock`]）。
pub(crate) fn request_named(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, CmdErr> {
    let mut v = machine::request(
        hosts,
        name,
        last_good::of(hosts, name).as_ref(),
        use_,
        extra,
    )?;
    if v.get("agent_sock").is_none_or(serde_json::Value::is_null) {
        if let Some(sock) = agent_sock::seen() {
            v["agent_sock"] = serde_json::json!(sock);
        }
    }
    Ok(v)
}

/// 同 [`request_named`]，读成拨号请求。
pub(crate) fn dial_named(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<DialRequest, CmdErr> {
    read_request(&request_named(hosts, name, use_, extra)?)
}

/// 同 [`request_named`]，两样都交：组好的那一份（问那台 · 存着再用）与读成的拨号请求（开会话用）。
pub(crate) fn named_both(
    hosts: &[Host],
    name: &str,
    use_: &str,
    extra: serde_json::Value,
) -> Result<(serde_json::Value, DialRequest), CmdErr> {
    let v = request_named(hosts, name, use_, extra)?;
    let req = read_request(&v)?;
    Ok((v, req))
}

/// 传输那一趟的拨号（[`sftp::Dial`]）：按名字照表组（同 [`named_both`]）。
pub(crate) fn sftp_named(
    hosts: &[Host],
    name: &str,
) -> Result<(serde_json::Value, sftp::Dial), String> {
    let (v, req) =
        named_both(hosts, name, "capture", serde_json::json!({})).map_err(|(_, said)| said)?;
    Ok((v, sftp::Dial::of(req)))
}

/// 一份组好的拨号请求（[`request_named`] 那一形）⇒ 拨号请求。
pub(crate) fn read_request(v: &serde_json::Value) -> Result<DialRequest, CmdErr> {
    <DialRequest as serde::Deserialize>::deserialize(v).map_err(|e| {
        (
            "bad_args",
            crate::common::contract::malformed(&format!("dial request unreadable: {e}")),
        )
    })
}

/// 还没存进表的那一台（测试连接 · 推公钥测的那一台）⇒ 组好的拨号请求：它顶替表里同名的那一台（没有就加上），跳板照表按名字找。
pub(crate) fn request_draft(
    hosts: &[Host],
    draft: &Host,
    use_: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, CmdErr> {
    let mut table: Vec<Host> = hosts
        .iter()
        .filter(|h| h.name() != draft.name())
        .cloned()
        .collect();
    table.push(draft.clone());
    request_named(&table, draft.name(), use_, extra)
}

/// `link-open` 的 `dial` 那一格：`{machine: "<那台的名字>", use?, …}`，其余格（`command` · `capture` · `forward` · `stages` · `probe` ·
/// `progress` · `agent_sock`）原样进请求；`use` 缺席 ⇒ `stream`。怎么拨（地址 · 端口 · 钥匙 · 跳板 · 先试哪条）只由这一端的机器表定：
/// `machine` 不是名字（整台配置）· 线上自带 `jump` / `prefer` ⇒ `bad_args`。回那台的名字与拨号请求。
pub(crate) fn parse_named(
    v: &serde_json::Value,
    hosts: &[Host],
) -> Result<(String, DialRequest), CmdErr> {
    let bad = |d: &str| ("bad_args", crate::common::contract::malformed(d));
    let obj = v.as_object().ok_or_else(|| bad("dial is not an object"))?;
    let name = match obj.get("machine") {
        Some(serde_json::Value::String(n)) if !n.is_empty() => n.clone(),
        Some(serde_json::Value::String(_)) | None => {
            return Err(bad("`machine` (the machine's name) is missing"))
        }
        Some(_) => return Err(bad("`machine` is the machine's name, not its settings")),
    };
    for k in ["jump", "prefer"] {
        if obj.contains_key(k) {
            return Err(bad(&format!(
                "`{k}` is not taken: the machine table says how to dial"
            )));
        }
    }
    let use_ = match obj.get("use") {
        None | Some(serde_json::Value::Null) => "stream".to_string(),
        Some(serde_json::Value::String(u)) => u.clone(),
        Some(_) => return Err(bad("`use` is not a string")),
    };
    if let Some(sock) = obj.get("agent_sock").and_then(serde_json::Value::as_str) {
        agent_sock::remember(sock);
    }
    let mut extra = obj.clone();
    for k in ["machine", "use"] {
        extra.remove(k);
    }
    let req = dial_named(hosts, &name, &use_, serde_json::Value::Object(extra))?;
    Ok((name, req))
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_tests.rs"]
mod tests;
