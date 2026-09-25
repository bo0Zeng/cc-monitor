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
//! 回环上连一次那个口：连得上 = **有人在听**。连不上 = 没人听（或连接被拒），那时才起。
//!
//! 〔RK1 · `INVARIANTS §48.1`〕中转口有了钥匙之后，「有人在听」**不够**：升级前起的旧 `--relay`（没钥匙）会一直占着那个口，
//! 注入带钥匙段的地址给它 ⇒ 每一发 404，与网络故障同形。⇒ 口上有人时再做一次**差分探针**（[`occupant`]）：
//! 读这台机器上的钥匙文件，带对的钥匙打一次 `GET /<钥匙>/`（我们的中转：钥匙过了、不是路由 ⇒ **404**），
//! 带一把同形但不对的打一次（我们的中转 ⇒ **403**）。两条都对上才算「我们的中转在听」；
//! 口上有人却对不上 ⇒ `not_ours`（出声）。钥匙**不出线**：只在这台后端里读文件、打回环，应答只有布尔与端口。
//! ⚠ 仍答不了的：一个**有意模仿**我们门的程序（它得先读得到那份 `0600` 的钥匙文件 —— 那时它本来就能以你的身份跑东西）。
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

/// `relay-status`：这个口上**我们的中转**在不在听。**只读**。
///
/// 〔RK1〕口上有人、但这台机器上还没有钥匙文件 ⇒ 答 `listening:false`（不是错）：
/// 刚起的中转在「绑上口」与「钥匙落盘」之间有一个窄窗，`relay-ensure` 之后的有界等待正好会问到它；
/// 真是别人占着的话，下一步 `relay-ensure` 会以 `not_ours` 说出来。
pub(crate) fn answer_status(args: &Value) -> MachineAnswer {
    answer_status_with(args, &|k| std::env::var(k).ok())
}

/// [`answer_status`] 的本体：环境取值器注入（判据喂夹具家目录，不读用户真实的那一份钥匙）。
pub(crate) fn answer_status_with(
    args: &Value,
    get: &dyn Fn(&str) -> Option<String>,
) -> MachineAnswer {
    let port = port_arg(args)?;
    let listening = match occupant(port, get) {
        Occupant::Nobody | Occupant::NoKeyFile => false,
        Occupant::Ours => true,
        Occupant::NotOurs(why) => return Err(("not_ours", why)),
    };
    Ok(json!({ "port": port, "listening": listening }))
}

/// `relay-ensure`：我们的中转在听 ⇒ 什么都不做；没人 ⇒ 起一个脱离的 `--relay`；口上是别的 ⇒ `not_ours`。
pub(crate) fn answer_ensure(args: &Value) -> MachineAnswer {
    answer_ensure_with(args, &|k| std::env::var(k).ok())
}

/// [`answer_ensure`] 的本体（同上，取值器注入）。
pub(crate) fn answer_ensure_with(
    args: &Value,
    get: &dyn Fn(&str) -> Option<String>,
) -> MachineAnswer {
    let port = port_arg(args)?;
    match occupant(port, get) {
        Occupant::Ours => {
            return Ok(json!({ "port": port, "listening": true, "started": false }));
        }
        Occupant::NotOurs(why) => return Err(("not_ours", why)),
        Occupant::NoKeyFile => {
            return Err((
                "not_ours",
                format!(
                    "{port} 口上有人在听，但这台机器上没有中转钥匙文件（我们的中转绑上口就会留下它）—— \
                     多半是升级前起的旧中转或别的程序。在那台上结束占着这个口的进程再试"
                ),
            ));
        }
        Occupant::Nobody => {}
    }
    let exe = std::env::current_exe()
        .map_err(|e| ("spawn_failed", format!("找不到本后端自己这个二进制：{e}")))?;
    let pid = start(&exe, &["--relay"], port)?;
    Ok(json!({ "port": port, "listening": false, "started": true, "pid": pid }))
}

/// 〔RK1〕口上是谁。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Occupant {
    /// 没人在听。
    Nobody,
    /// 有人在听，而这台机器上没有（形状对的）钥匙文件。
    NoKeyFile,
    /// 我们的中转：对的钥匙 ⇒ 404、错的钥匙 ⇒ 403。
    Ours,
    /// 有人在听，但差分探针对不上（串是给人看的为什么）。
    NotOurs(String),
}

/// 探针那一次读写最多等多久（回环上对端一个字节都不回就会永远挂住；`no_timer_guard` 登记：一次阻塞的上限，不是定时器）。
const PROBE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(2_000);

/// 〔RK1〕口上是谁（射程见模块头注）。取值器注入 ⇒ 判据喂夹具家目录。
pub(crate) fn occupant(port: u16, get: &dyn Fn(&str) -> Option<String>) -> Occupant {
    if !listening(port) {
        return Occupant::Nobody;
    }
    let Some(key) = super::door::key_path(get).and_then(|p| super::door::read_key(&p)) else {
        return Occupant::NoKeyFile;
    };
    let right = key.expose();
    // 同形但不对：只换最后一个字符。
    let last = if right.ends_with('0') { "1" } else { "0" };
    let wrong = format!("{}{last}", &right[..right.len() - 1]);
    match (probe(port, right), probe(port, &wrong)) {
        (Ok(404), Ok(403)) => Occupant::Ours,
        (a, b) => Occupant::NotOurs(format!(
            "{port} 口上有人在听，但它不认这台机器上的中转钥匙（对的钥匙得 {}，错的得 {}；我们的中转应是 404 / 403）—— \
             多半是升级前起的旧中转或别的程序。在那台上结束占着这个口的进程再试",
            said(&a),
            said(&b)
        )),
    }
}

fn said(r: &std::io::Result<u16>) -> String {
    match r {
        Ok(code) => code.to_string(),
        Err(e) => format!("没回（{e}）"),
    }
}

/// 打一发 `GET /<段>/`，读回状态码。**请求里的段永远不进任何日志 / 应答**（本函数只回一个数）。
fn probe(port: u16, seg: &str) -> std::io::Result<u16> {
    use std::io::{BufRead, Write};
    let mut s = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    s.set_read_timeout(Some(PROBE_DEADLINE))?;
    s.set_write_timeout(Some(PROBE_DEADLINE))?;
    s.write_all(
        format!("GET /{seg}/ HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )?;
    let mut line = String::new();
    std::io::BufReader::new(s).read_line(&mut line)?;
    line.split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| std::io::Error::other("回的第一行不是 HTTP 状态行"))
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
