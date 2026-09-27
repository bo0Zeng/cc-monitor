//! 〔HOST · V139〕**远端常驻后端的起 · 找 · 停**（`99 §1` V139「远端常驻、本机远端同形」· `设计/01 §3.3` · `05 §5.2`）。
//!
//! 两条一次性子命令，由 monitor 经本机常驻后端的链路 `capture` 在远端跑（设计住仓外 `调研/第四波记录/HOST.md §1`）：
//! - `--resident-ensure [--replace]`：读回或铸这台的监听钥匙（`~/.cc-monitor/listen-token`，与本机宿主同一份文件）→
//!   **无条件起一个脱离的自己**（常驻载体；钥匙经文件交，不进 env / argv）→ 回一行 `{"port","token","pid"}`。
//!   口上已有常驻后端时，子进程照旧「绑不上就退 3、绝不换口」⇒ 找与起是同一步，本进程不等任何东西（零定时器）。
//!   `--replace`：先按口上那一位自己记的 pid 文件发 SIGTERM（它排空后退），再起（只升不降由 monitor 按 hello 判）。
//! - `--resident-stop`：同上那一枪，不起。回 `{"stopped":pid|null}`。
//!
//! 口按 agent 家目录算（共享 crate `relay_route_core::listen_port_for`，本机宿主同一个函数）⇒ 一台机器一个常驻后端。
//! ⚠ 钥匙会出现在 `--resident-ensure` 的 stdout 上：那一行只走 SSH 通道到 monitor 内存，不进日志（调用侧不许打印它）。

use std::path::{Path, PathBuf};

use copy_core::copy_text;

/// `--resident-ensure` 起子进程时给的流模式默认旗标（空转那份 watcher 用；每条连接按 attach 行自己的 `flags`）。
/// 与本机宿主 `LOCAL_STREAM_ARGS` 同一组。
pub(crate) const DEFAULT_STREAM_ARGS: &[&str] = &["--tail-only", "--with-bg", "--with-rbind-token"];

/// 钥匙字节数：16 字节 = 128 位（`INVARIANTS §48.1`「新生成时 128 位随机」），落盘 32 个小写十六进制字符（同本机宿主）。
const TOKEN_BYTES: usize = 16;

/// 远端常驻后端自己的 stderr 诊断文件（本机那一份由宿主交数据目录下的路径）；env 名由 `main.rs` 交（诊断文件的门在那里）。
pub const STDERR_LOG_REL: &str = ".cc-monitor/resident-stderr.log";

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn pid_path(home: &Path, port: u16) -> PathBuf {
    home.join(".cc-monitor").join(format!("listen-{port}.pid"))
}

/// 这台机器的常驻监听口（按 agent 家目录，与本机宿主同一个函数）。
pub(crate) fn port_for(agent_home: &Path) -> u16 {
    relay_route_core::listen_port_for(&agent_home.to_string_lossy())
}

/// 一次性子命令的错误出口：stderr 一行 `{code,message}`、退出 2（协议 v1 §3）—— 与 CLI 控制面同一份信封。
fn fail(code: &str, message: String) -> i32 {
    super::cli_control::emit_err(code, message)
}

/// `--resident-ensure [--replace]`。`hosted`：宿主层（`main.rs`）交的额外环境 —— V139 的中转口那一格
/// （本层不许伸手进 `relay/`，`layering_guard`）。
pub fn run_ensure(agent_home: &Path, args: &[String], hosted: &[(&str, String)]) -> i32 {
    let Some(home) = home() else {
        return fail("no_home", copy_text("beResident.home.missing", &[]));
    };
    let port = port_for(agent_home);
    let token_path = home.join(relay_route_core::LISTEN_TOKEN_FILE_REL);
    let token = match ensure_token(&token_path) {
        Ok(t) => t,
        Err(e) => return fail("no_token", e),
    };
    if args.iter().any(|a| a == "--replace") {
        if let Err(e) = terminate_owner(&home, port) {
            return fail("replace_failed", e);
        }
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
            println!(
                "{}",
                serde_json::json!({ "port": port, "token": token, "pid": pid })
            );
            0
        }
        Err((code, e)) => fail(code, e),
    }
}

/// `--resident-ensure` 的入口：宿主层环境 = 中转口（V139）· stderr 诊断文件 ·
/// 〔TAIL · HOST 余项〕数据目录那两格按默认推（谁起都一样）⇒ 那台自己的 monitor 能收养它。
pub fn ensure(agent_home: &Path, args: &[String]) -> i32 {
    let mut hosted = vec![
        (
            crate::listen::RELAY_PORT_ENV,
            relay_route_core::PORT.to_string(),
        ),
        (crate::stderr_log::ENV, format!("~/{STDERR_LOG_REL}")),
    ];
    let [_, creds_env, meta_env] = crate::wire::HOST_ECHO_ENVS;
    if let Some(h) = home() {
        hosted.extend(data_dir_envs(
            &|k| std::env::var(k).ok(),
            &h,
            creds_env,
            meta_env,
        ));
    }
    run_ensure(agent_home, args, &hosted)
}

/// `--resident-stop`。
pub fn run_stop(agent_home: &Path, _args: &[String]) -> i32 {
    let Some(home) = home() else {
        return fail("no_home", copy_text("beResident.home.missing", &[]));
    };
    match terminate_owner(&home, port_for(agent_home)) {
        Ok(pid) => {
            println!("{}", serde_json::json!({ "stopped": pid }));
            0
        }
        Err(e) => fail("stop_failed", e),
    }
}

/// 读回钥匙；没有 / 空 ⇒ 在目录锁里再读一次，仍没有才铸（与中转钥匙同形：`relay/door.rs::ensure_key`）。
fn ensure_token(path: &Path) -> Result<String, String> {
    let read = || {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    if let Some(t) = read() {
        return Ok(t);
    }
    let dir = path.parent().ok_or_else(|| {
        copy_text(
            "beResident.fs.noParent",
            &[("path", &path.display().to_string())],
        )
    })?;
    ensure_dir(dir)?;
    let _lock = crate::platform::lock::hold(dir)?;
    if let Some(t) = read() {
        return Ok(t);
    }
    let t = mint()?;
    write_private(path, &t)?;
    Ok(t)
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    crate::own_dir::ensure_private_dir(dir).map_err(|e| {
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

/// 〔TAIL · HOST 余项〕数据目录那两格（凭据文件 · 历史注解）的默认值 —— 与那台 monitor 自己算的是同一条规矩
/// （`creds_core::store::monitor_data_dir`）⇒ 谁起的常驻后端，hello 回显的都是同一对值，那台自己的 monitor 能收养（HX2 不拒）。
/// 本进程环境里已有的那一格不覆盖；推不出来（`CCM_DATA_DIR` 不是绝对路径）⇒ 两格都缺席。纯函数。
pub fn data_dir_envs(
    get: &dyn Fn(&str) -> Option<String>,
    home: &Path,
    creds_env: &'static str,
    meta_env: &'static str,
) -> Vec<(&'static str, String)> {
    use creds_core::store::{monitor_data_dir, DATA_DIR_ENV, FILE_NAME, HISTORY_METADATA_FILE};
    let Some(dir) = monitor_data_dir(get(DATA_DIR_ENV).as_deref(), Some(home.to_path_buf())) else {
        return Vec::new();
    };
    [(creds_env, FILE_NAME), (meta_env, HISTORY_METADATA_FILE)]
        .into_iter()
        .filter(|(k, _)| get(k).filter(|v| !v.is_empty()).is_none())
        .map(|(k, f)| (k, dir.join(f).display().to_string()))
        .collect()
}

/// 子进程的环境：口 · 钥匙文件路径（不是钥匙）· 宿主层交的那几格（V139 中转口 · stderr 诊断文件，值里的 `~` 换成家目录）。
/// 纯函数，判据钉它。
pub(crate) fn child_env(
    port: u16,
    token_path: &Path,
    home: &Path,
    hosted: &[(&str, String)],
) -> Vec<(String, String)> {
    let mut env = vec![
        (crate::listen::ENV_PORT.into(), port.to_string()),
        (
            crate::listen::ENV_TOKEN_FILE.into(),
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
    let mut cmd = std::process::Command::new(exe);
    cmd.args(DEFAULT_STREAM_ARGS)
        .env_remove("TMUX")
        .env_remove(crate::listen::ENV_TOKEN)
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

/// 常驻后端绑上口之后记下「谁在听」（`pid\n二进制\n`，与本机宿主写的同形）—— 远端起它的那一方不在场，由它自己记。
pub fn record_owner(port: u16) -> Result<(), String> {
    let home = home().ok_or_else(|| copy_text("beResident.home.missing", &[]))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = pid_path(&home, port);
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

/// 按 pid 文件给口上那一位发 SIGTERM（先核身份：pid 会被复用）。没有记录 / 进程已不在 ⇒ `Ok(None)`。
fn terminate_owner(home: &Path, port: u16) -> Result<Option<u32>, String> {
    let Some((pid, bin)) = std::fs::read_to_string(pid_path(home, port))
        .ok()
        .and_then(|b| parse_owner(&b))
    else {
        return Ok(None);
    };
    if !crate::platform::proc::pid_alive(pid) {
        return Ok(None);
    }
    let seen = crate::platform::proc::exe_of(pid)
        .ok_or_else(|| copy_text("beResident.stop.cannotVerify", &[("pid", &pid.to_string())]))?;
    if !exe_matches(&seen, &bin) {
        return Err(copy_text(
            "beResident.stop.notOurs",
            &[("pid", &pid.to_string()), ("exe", &seen)],
        ));
    }
    if crate::platform::signal::send_sigterm(pid) {
        Ok(Some(pid))
    } else {
        Err(copy_text(
            "beResident.stop.signalFailed",
            &[("pid", &pid.to_string())],
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/resident_tests.rs"]
mod tests;
