//! 〔RM1a · 第四波〕中转 · **这台机器上的中转进程**：口上有没有人在听 · 没有就起一个脱离的。
//!
//! # 它补的是哪一格
//!
//! 〔RL1 · V107〕本机的中转住**本机常驻后端进程里**（`listen::host`，monitor 起本机后端时交端口）。
//! 远端那台机器上的后端经 SSH exec 起、随 SSH 一起退，而远端会话活在 tmux 里、比 SSH 长 ——
//! 中转要活得比 SSH 长 ⇒ 远端那一个是本模块起的**脱离的** `--relay`（帧面 `relay-ensure`），
//! 由那台后端答「在不在」（帧面 `relay-status`）。远端有了常驻监听口（`设计/05 §5.2`）那一天，两边才收成同一形。
//!
//! # 🔴 它是**中转**的事，一个上游选择的名字都没有
//!
//! 起的是 `--relay` 这个进程（它里面的上游选择由 `main.rs` 那一臂装配，与本模块无关）；
//! 交给它的只有端口。凭据文件在哪**不由这里说**：起出来的中转继承本后端的环境，
//! 它里面的上游选择与本后端账号域那份写口按**同一个函数、同一个家目录出处**解路径
//! （`accounts::upstream::file_face` 头注）⇒ 两边是同一份文件，这里不必认识它的任何名字
//! （`upstream_selection_guard` ㈢ 照样零命中）。
//!
//! # 判「在不在」的射程（照实写）
//!
//! 回环上连一次那个口：连得上 = **有人在听**。⚠ 它答不了「听的那个是不是我们的中转」——
//! 那要一次 HTTP 往返且中转今天没有自报身份的端点。连不上 = 没人听（或连接被拒），那时才起。
//!
//! # 起法
//!
//! 本后端这个二进制自己（`current_exe`）带 `--relay`，环境多一格端口；stdio 全接空
//! （远端后端走 SSH 管道，子进程若继承那几根管道，SSH 一断它写诊断就会被信号打死）；
//! 放进自己的进程组（`platform::detach`）。**不等它 bind**（后端零定时器）⇒ `started: true`
//! 只说「进程起了」，要知道口上有没有人，再问一次 `relay-status`。
//! ⚠ 它的诊断（起不来 · 凭据文件在哪 · 表里几行）因此**到不了人** —— 远端那台上没有监护者收它的 stderr。

use serde_json::{json, Value};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type MachineAnswer = Result<Value, (&'static str, String)>;

/// `relay-status`：这个口上有没有人在听。**只读**。
pub(crate) fn answer_status(args: &Value) -> MachineAnswer {
    let port = port_arg(args)?;
    Ok(json!({ "port": port, "listening": listening(port) }))
}

/// `relay-ensure`：有人在听 ⇒ 什么都不做；没人 ⇒ 起一个脱离的 `--relay`。
pub(crate) fn answer_ensure(args: &Value) -> MachineAnswer {
    let port = port_arg(args)?;
    if listening(port) {
        return Ok(json!({ "port": port, "listening": true, "started": false }));
    }
    let exe = std::env::current_exe()
        .map_err(|e| ("spawn_failed", format!("找不到本后端自己这个二进制：{e}")))?;
    let pid = start(&exe, &["--relay"], port)?;
    Ok(json!({ "port": port, "listening": false, "started": true, "pid": pid }))
}

/// 回环上连一次那个口。连得上 = 有人在听（射程见模块头注）。
pub(crate) fn listening(port: u16) -> bool {
    TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok()
}

/// 起一个脱离的子进程：`program args…`，环境多一格端口，stdio 全接空，自成一个进程组。
///
/// 抽成带 `program` / `args` 的函数，是为了让判据拿**测试二进制自己**当那个子进程
/// 走**这一份**起法（不是一份同构的副本）。回子进程的 pid。
pub(crate) fn start(
    program: &Path,
    args: &[&str],
    port: u16,
) -> Result<u32, (&'static str, String)> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .env(super::listen::ENV_PORT, port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::platform::detach::detach(&mut cmd).map_err(|e| ("unsupported", e))?;
    cmd.spawn().map(|c| c.id()).map_err(|e| {
        (
            "spawn_failed",
            format!("起 {} 失败：{e}", program.display()),
        )
    })
}

/// `args.port`：1–65535。
fn port_arg(args: &Value) -> Result<u16, (&'static str, String)> {
    args.get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p != 0)
        .ok_or(("bad_args", "缺 `port`，或它不是 1–65535 的整数".to_string()))
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/machine_tests.rs"]
mod tests;
