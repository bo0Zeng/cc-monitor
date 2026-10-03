//! **远端常驻后端的起 · 找 · 停**（「远端常驻、本机远端同形」·）。
//!
//! 两条一次性子命令，由 monitor 经本机常驻后端的链路 `capture` 在远端跑（设计住仓外）：
//! - `--resident-ensure [--replace]`：口上已有人在听 ⇒ 不再起，回盘上那把钥匙（`~/.cc-monitor/listen-token`，它起来时写的）；
//!   没人 ⇒ 起一个脱离的自己（常驻载体；钥匙文件只交路径），回一行 `{"port","token","pid"}`，`token` 为 `null`：
//!   钥匙由那个常驻后端绑上口之后自己写（[`rotate_token`]），客户端读到 hello 之后再问一次就拿得到。本进程不等任何东西（零定时器）。
//!   `--replace`：先按下面那个停法停掉口上那一位（它自己记的 pid 文件），再起（只升不降由 monitor 按 hello 判）。
//! - `--resident-stop [--grace <秒>]`：**同机监督者**那一形（k8s `terminationGracePeriodSeconds` · systemd `TimeoutStopSec`）：
//!   SIGTERM（它按 HX1 排空后自己退）→ 在宽限期内等内核通知 → 到点 SIGKILL → 回 `{"stopped":"graceful"|"killed"|"not_running","pid":n|null}`。
//!   等待住这个一次性进程里，常驻后端的事件循环不加定时器（`no_timer_guard::REGISTERED_ONE_SHOT_CLI_WAITS`）。
//!
//! 口按这台的家算（`~/.cc-monitor`，隔离跑时 `CCM_DATA_DIR`；共享 crate `relay_route_core::listen_port_for`，本机宿主同一个函数）
//! ⇒ 一台机器一个常驻后端，与 Claude 目录、与哪一家 agent 都无关。钥匙与「谁在听」也住这个家，都由常驻后端自己写（本机远端一个写者）。
//! ⚠ 钥匙会出现在 `--resident-ensure` 的 stdout 上：那一行只走 SSH 通道到 monitor 内存，不进日志（调用侧不许打印它）。

use crate::common::child_env::WithoutOwnEnv;
use std::path::{Path, PathBuf};

use copy_core::copy_text;

/// `--resident-ensure` 起子进程时给的流模式默认旗标（空转那份 watcher 用；每条连接按 attach 行自己的 `flags`）。
/// 与本机宿主 `LOCAL_STREAM_ARGS` 同一组。
/// 流模式显式词打头（本二进制就叫 `ccm` 时零参数是起会话；这里其实已有 `--tail-only` 打头，带上它是为了与宿主那组同形）。
pub(crate) const DEFAULT_STREAM_ARGS: &[&str] = &[
    // 打头的 `--`：后面是后端的词（本二进制就叫 `ccm`，没有它整行交给 claude）。
    "--",
    crate::STREAM_FLAG_EXPLICIT,
    "--tail-only",
    "--with-bg",
    "--with-pid",
];

/// 宽限期默认值（毫秒）。**必须大于**常驻后端自己的退出排空上限（`inbound::DRAIN_DEADLINE`）：
/// 后端先把「哪几条没做完」说出来、自己退；强杀只兜它连那一步都走不到的情形。关系由 `resident_tests` 钉住。
pub(crate) const STOP_GRACE_MS: u32 = 35_000;
/// 强杀之后再等它没了的上限（毫秒）：SIGKILL 是立刻的，还在 ⇒ 卡在内核里（D 态），如实报「没停掉」。
pub(crate) const KILL_WAIT_MS: u32 = 5_000;
/// `--grace` 的上限（秒）：再大就没有意义（monitor 那头一次性那一趟的期限先到）。
const GRACE_MAX_SECS: u32 = 3_600;

/// 钥匙字节数：16 字节 = 128 位（`INVARIANTS §48.1`「新生成时 128 位随机」），落盘 32 个小写十六进制字符（同本机宿主）。
const TOKEN_BYTES: usize = 16;

/// 远端常驻后端自己的 stderr 诊断文件（本机那一份由宿主交数据目录下的路径）；env 名由 `main.rs` 交（诊断文件的门在那里）。
/// 与本机同一层级 `logs/backend/stderr.log`（滚出来的旧那份在同目录 `stderr.old.log`）。
pub const STDERR_LOG_REL: &str = ".cc-monitor/logs/backend/stderr.log";

fn home() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 这台的家：门牌（口 · 钥匙 · 谁在听）只跟着它走。规矩与 monitor 同一份（`creds_core::store::monitor_data_dir`）。
fn data_home() -> Option<PathBuf> {
    creds_core::store::monitor_data_dir(
        std::env::var(creds_core::store::DATA_DIR_ENV)
            .ok()
            .as_deref(),
        home(),
    )
}

fn pid_path(data_home: &Path, port: u16) -> PathBuf {
    data_home.join(relay_route_core::listen_pid_file_name(port))
}

fn token_path(data_home: &Path) -> PathBuf {
    data_home.join(relay_route_core::file_name_of(
        relay_route_core::LISTEN_TOKEN_FILE_REL,
    ))
}

/// 一次性子命令的错误出口：stderr 一行 `{code,message}`、退出 2（协议 v1 §3）—— 与 CLI 控制面同一份信封。
fn fail(code: &str, message: String) -> i32 {
    super::cli_control::emit_err(code, message)
}

/// `--resident-ensure [--replace]`。`hosted`：宿主层（`main.rs`）交的额外环境 —— 中转口那一格
/// （本层不许伸手进 `relay/`，`layering_guard`）。
pub fn run_ensure(args: &[String], hosted: &[(&str, String)]) -> i32 {
    let (Some(home), Some(dh)) = (home(), data_home()) else {
        return fail("no_home", copy_text("beResident.home.missing", &[]));
    };
    let port = relay_route_core::listen_port_for(&dh);
    let token_path = token_path(&dh);
    if args.iter().any(|a| a == "--replace") {
        // 旧的不先让出口，新的必然绑不上 ⇒ 与「停」同一个停法（等它真退了再起）。
        if let Err(e) = stop_owner(&dh, port, STOP_GRACE_MS) {
            return fail("replace_failed", e);
        }
    } else if someone_listening(port) {
        // 口上已有人 ⇒ 不再起（起了也绑不上）；它认的是它起来时写下的那一把，读盘上那一份。
        return match crate::stream::listen::token_from_file(&token_path.display().to_string()) {
            Ok(token) => {
                println!(
                    "{}",
                    serde_json::json!({ "port": port, "token": token, "pid": null })
                );
                0
            }
            Err(e) => fail("no_token", e),
        };
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return fail(
                "spawn_failed",
                copy_text("beResident.spawn.noSelf", &[("e", &e.to_string())]),
            )
        }
    };
    match spawn_detached(&exe, &child_env(port, &token_path, &home, hosted)) {
        Ok(pid) => {
            // 钥匙由它绑上口之后自己写：此刻还没有，客户端读到 hello 之后再问一次。
            println!(
                "{}",
                serde_json::json!({ "port": port, "token": null, "pid": pid })
            );
            0
        }
        Err((code, e)) => fail(code, e),
    }
}

/// 这台回环上那个口此刻有没有人在听（连一下：回环上「没人」是立刻被拒，不等）。
fn someone_listening(port: u16) -> bool {
    std::net::TcpStream::connect((crate::stream::listen::LOOPBACK, port)).is_ok()
}

/// `--resident-ensure` 的入口：宿主层环境 = 中转口 · stderr 诊断文件（凭据与历史注解住家里，那台后端按家自己推）。
pub fn ensure(args: &[String]) -> i32 {
    let hosted = vec![
        // 中转口与这台 `ccm` 起会话时找的是同一个口（同一个函数：这台环境里交了就用交的，否则默认口）。
        (
            crate::stream::listen::RELAY_PORT_ENV,
            crate::accounts::upstream_select::endpoint::relay_port(&|k| std::env::var(k).ok())
                .to_string(),
        ),
        (crate::stderr_log::ENV, format!("~/{STDERR_LOG_REL}")),
    ];
    if let Some(h) = home() {
        // 诊断文件那层目录先建好（`stderr_log` 只 `O_EXCL` 建文件、不建目录）；建不了 ⇒ 子进程装不上、stderr 照旧 null，不拖垮起。
        let _ = log_dir_chain(&h);
    }
    run_ensure(args, &hosted)
}

/// `--resident-stop [--grace <秒>]`。
pub fn run_stop(args: &[String]) -> i32 {
    let Some(dh) = data_home() else {
        return fail("no_home", copy_text("beResident.home.missing", &[]));
    };
    let grace_ms = match parse_grace(args) {
        Ok(g) => g,
        Err(e) => return fail("bad_args", e),
    };
    match stop_owner(&dh, relay_route_core::listen_port_for(&dh), grace_ms) {
        Ok(end) => {
            println!(
                "{}",
                serde_json::json!({ "stopped": end.word(), "pid": end.pid() })
            );
            0
        }
        Err(e) => fail("stop_failed", e),
    }
}

/// `--grace <秒>` ⇒ 毫秒；不给 ⇒ [`STOP_GRACE_MS`]。不大于排空上限的拒：那样后端还没说完就被强杀（纯函数）。
pub(crate) fn parse_grace(args: &[String]) -> Result<u32, String> {
    let Some(i) = args.iter().position(|a| a == "--grace") else {
        return Ok(STOP_GRACE_MS);
    };
    let drain_secs =
        u32::try_from(crate::stream::inbound::DRAIN_DEADLINE.as_secs()).unwrap_or(u32::MAX);
    let bad = || {
        copy_text(
            "beResident.stop.badGrace",
            &[
                ("min", &(drain_secs + 1).to_string()),
                ("max", &GRACE_MAX_SECS.to_string()),
            ],
        )
    };
    let secs: u32 = args
        .get(i + 1)
        .and_then(|v| v.parse().ok())
        .ok_or_else(bad)?;
    if secs <= drain_secs || secs > GRACE_MAX_SECS {
        return Err(bad());
    }
    Ok(secs * 1000)
}

/// 「停」的结局（线上三个词）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stopped {
    /// 收到 SIGTERM 之后在宽限期内自己退了。
    Graceful(u32),
    /// 宽限期满还在 ⇒ 强杀了，而且它确实没了。
    Killed(u32),
    /// 没有记录，或记下的那个已经不在。
    NotRunning,
}

impl Stopped {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Graceful(_) => "graceful",
            Self::Killed(_) => "killed",
            Self::NotRunning => "not_running",
        }
    }
    pub(crate) fn pid(self) -> Option<u32> {
        match self {
            Self::Graceful(p) | Self::Killed(p) => Some(p),
            Self::NotRunning => None,
        }
    }
}

/// `~/.cc-monitor` → `logs` → `backend` 逐层建（每层 0700，已在的不动）。
fn log_dir_chain(home: &Path) -> Result<(), String> {
    let Some(dir) = home.join(STDERR_LOG_REL).parent().map(Path::to_path_buf) else {
        return Ok(());
    };
    let mut chain: Vec<&Path> = dir.ancestors().take_while(|a| *a != home).collect();
    chain.reverse();
    chain.into_iter().try_for_each(ensure_dir)
}

/// 常驻后端**绑上口之后**换一把新钥匙写回钥匙文件（临时件出生即只给本人 → 写满 → 原子挪过去），回这一把。
/// 每次起都换 ⇒ 旧环境里漏出去的那把随之作废；抢不到口的后起者走不到这里（不碰在跑那一个的钥匙）。
/// 本机远端都由常驻后端自己写（起它的那一方只交路径）；连上来的客户端每次读文件。
pub fn rotate_token(path: &Path) -> Result<String, String> {
    if let Some(dir) = path.parent() {
        ensure_dir(dir)?;
    }
    let t = mint()?;
    write_private(path, &t)?;
    Ok(t)
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        copy_text(
            "beResident.fs.mkdirFailed",
            &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
        )
    })
}

fn mint() -> Result<String, String> {
    let mut buf = [0u8; TOKEN_BYTES];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut buf)
        .map_err(|_| copy_text("beResident.token.noRandom", &[]))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// 临时文件出生即只给本人（`O_EXCL`）→ 写满 → 原子挪过去；失败删自己的临时文件。报错里只有路径。
fn write_private(path: &Path, body: &str) -> Result<(), String> {
    use std::io::Write as _;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let (t, p) = (tmp.display().to_string(), path.display().to_string());
        let mut f = creds_core::perm::create_private(&tmp).map_err(|e| {
            copy_text(
                "beResident.fs.tmpCreateFailed",
                &[("tmp", &t), ("e", &e.to_string())],
            )
        })?;
        f.write_all(body.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| {
                copy_text(
                    "beResident.fs.tmpWriteFailed",
                    &[("tmp", &t), ("e", &e.to_string())],
                )
            })?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| {
            copy_text(
                "beResident.fs.renameFailed",
                &[("tmp", &t), ("path", &p), ("e", &e.to_string())],
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 子进程的环境：口 · 钥匙文件路径（不是钥匙）· 宿主层交的那几格（中转口 · stderr 诊断文件，值里的 `~` 换成家目录）。
/// 纯函数，判据钉它。
pub(crate) fn child_env(
    port: u16,
    token_path: &Path,
    home: &Path,
    hosted: &[(&str, String)],
) -> Vec<(String, String)> {
    let mut env = vec![
        (crate::stream::listen::ENV_PORT.into(), port.to_string()),
        (
            crate::stream::listen::ENV_TOKEN_FILE.into(),
            token_path.display().to_string(),
        ),
    ];
    env.extend(hosted.iter().map(|(k, v)| {
        let v = match v.strip_prefix("~/") {
            Some(rel) => home.join(rel).display().to_string(),
            None => v.clone(),
        };
        (k.to_string(), v)
    }));
    env
}

/// 起一个脱离的自己（常驻载体）：stdio 全空（SSH 断了它不跟着收 SIGPIPE）、自成进程组、不继承 `TMUX`。
fn spawn_detached(exe: &Path, env: &[(String, String)]) -> Result<u32, (&'static str, String)> {
    let mut cmd = std::process::Command::new(exe).without_own_env();
    cmd.args(DEFAULT_STREAM_ARGS)
        .env_remove("TMUX")
        .envs(env.iter().cloned())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::platform::detach::detach(&mut cmd).map_err(|e| ("unsupported", e))?;
    cmd.spawn().map(|c| c.id()).map_err(|e| {
        (
            "spawn_failed",
            copy_text(
                "beResident.spawn.failed",
                &[("exe", &exe.display().to_string()), ("e", &e.to_string())],
            ),
        )
    })
}

/// 常驻后端绑上口之后记下「谁在听」（`pid\n二进制\n`）—— 本机远端都由它自己记（起它的那一方不写这份）。
pub fn record_owner(port: u16) -> Result<(), String> {
    let dh = data_home().ok_or_else(|| copy_text("beResident.home.missing", &[]))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = pid_path(&dh, port);
    if let Some(dir) = path.parent() {
        ensure_dir(dir)?;
    }
    write_private(
        &path,
        &format!("{}\n{}\n", std::process::id(), exe.display()),
    )
}

/// 解 pid 文件那两行（纯函数）。
pub(crate) fn parse_owner(body: &str) -> Option<(u32, PathBuf)> {
    let mut lines = body.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let bin = lines.next()?.trim();
    (pid != 0 && !bin.is_empty()).then(|| (pid, PathBuf::from(bin)))
}

/// `/proc/<pid>/exe` 读出来的路径是不是那个二进制：部署换过文件之后旧进程读出来带 ` (deleted)`，照样算（纯函数）。
pub(crate) fn exe_matches(seen: &str, recorded: &Path) -> bool {
    let seen = seen.strip_suffix(" (deleted)").unwrap_or(seen);
    Path::new(seen) == recorded
}

/// 按 pid 文件停口上那一位。没有记录 ⇒ `NotRunning`。
fn stop_owner(data_home: &Path, port: u16, grace_ms: u32) -> Result<Stopped, String> {
    let path = pid_path(data_home, port);
    let Some((pid, bin)) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|b| parse_owner(&b))
    else {
        return Ok(Stopped::NotRunning);
    };
    let end = stop_pid(pid, &bin, grace_ms, KILL_WAIT_MS)?;
    // 它已不在 ⇒ 那份记录是陈的：还指着它就收掉（下一个起来的会自己再记；指着别人的不动）。
    if std::fs::read_to_string(&path)
        .ok()
        .and_then(|b| parse_owner(&b))
        .is_some_and(|(p, _)| p == pid)
    {
        let _ = std::fs::remove_file(&path);
    }
    Ok(end)
}

/// 同机监督者本体：先拿进程把手、再核身份（pid 会被复用；拿到把手之后信号只打得到它）→ SIGTERM →
/// 至多等 `grace_ms` → 还在 ⇒ 强杀 → 至多再等 `kill_wait_ms`；还在 ⇒ `Err`（不说「停了」）。
pub(crate) fn stop_pid(
    pid: u32,
    bin: &Path,
    grace_ms: u32,
    kill_wait_ms: u32,
) -> Result<Stopped, String> {
    let Some(target) = crate::platform::signal::stoppable(pid)? else {
        return Ok(Stopped::NotRunning);
    };
    let seen = crate::platform::proc::exe_of(pid)
        .ok_or_else(|| copy_text("beResident.stop.cannotVerify", &[("pid", &pid.to_string())]))?;
    if !exe_matches(&seen, bin) {
        return Err(copy_text(
            "beResident.stop.notOurs",
            &[("pid", &pid.to_string()), ("exe", &seen)],
        ));
    }
    match target.ask_to_finish() {
        Ok(()) => {
            if target.exited_within(grace_ms)? {
                return Ok(Stopped::Graceful(pid));
            }
        }
        // 发不出「请你收尾」（Windows 没有这一格）⇒ 等也等不来它自己退，直接强杀、如实报「强杀」。
        Err(e) => tracing::warn!("{e}"),
    }
    target.kill()?;
    if target.exited_within(kill_wait_ms)? {
        Ok(Stopped::Killed(pid))
    } else {
        Err(copy_text(
            "beResident.stop.stuck",
            &[("pid", &pid.to_string())],
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/resident_tests.rs"]
mod tests;
