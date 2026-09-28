//! 〔RM1a · RK1 · US1 · DEL〕中转 · **这台机器上我们的中转在不在听**（只读，只收端口、只回布尔）。
//!
//! 中转只住常驻后端进程里（本机远端同形，V107 · V139；`listen::host`）。上游选择出成品时问这一句
//! （`launch-endpoint` 的 `listening` · `apikey-routing` 的 `running`，`设计/20 §3.3`）。
//! 〔DEL〕先前这里还起脱离的 `--relay`、答帧面 `relay-status` / `relay-ensure`（远端回落那一形）—— 随那一形删了。
//!
//! # 判「在不在」的射程（照实写）
//!
//! 回环上连一次那个口：连不上 = 没人听 ⇒ `false`。连得上**不够**：口上可能是别的程序（例如升级前起的、没钥匙的旧中转）。
//! ⇒ 再做一次**差分探针**（[`ours`]）：读这台机器上的钥匙文件，带对的钥匙打一次 `GET /<钥匙>/`（我们的中转：钥匙过了、
//! 不是路由 ⇒ **404**），带一把同形但不对的打一次（我们的中转 ⇒ **403**）。两条都对上才算「我们的中转在听」。
//! 钥匙**不出线**：只在这台后端里读文件、打回环，答的只有布尔。
//! ⚠ 仍答不了的：一个**有意模仿**我们门的程序（它得先读得到那份 `0600` 的钥匙文件 —— 那时它本来就能以你的身份跑东西）。

use std::net::{Ipv4Addr, SocketAddr, TcpStream};

/// 探针那一次读写最多等多久（回环上对端一个字节都不回就会永远挂住；`no_timer_guard` 登记：一次阻塞的上限，不是定时器）。
const PROBE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(2_000);

/// 这台机器上**我们的**中转在不在听。读者：上游选择出的两份成品（`launch-endpoint` 的 `listening` · `apikey-routing` 的 `running`）。
pub(crate) fn our_relay_listening(port: u16) -> bool {
    ours(port, &|k| std::env::var(k).ok())
}

/// [`our_relay_listening`] 的本体（射程见模块头注）。取值器注入 ⇒ 判据喂夹具家目录，不读用户真实的那一份钥匙。
pub(crate) fn ours(port: u16, get: &dyn Fn(&str) -> Option<String>) -> bool {
    if !listening(port) {
        return false;
    }
    let Some(key) = super::door::key_path(get).and_then(|p| super::door::read_key(&p)) else {
        return false;
    };
    let right = key.expose();
    // 同形但不对：只换最后一个字符。
    let last = if right.ends_with('0') { "1" } else { "0" };
    let wrong = format!("{}{last}", &right[..right.len() - 1]);
    matches!(
        (probe(port, right), probe(port, &wrong)),
        (Ok(404), Ok(403))
    )
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
        .ok_or_else(|| std::io::Error::other("first reply line is not an HTTP status line"))
}

/// 回环上连一次那个口。连得上 = 有人在听（射程见模块头注）。
pub(crate) fn listening(port: u16) -> bool {
    TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok()
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/machine_tests.rs"]
mod tests;
