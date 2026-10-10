//! 帧命令 `session-terminals`：**此刻是哪个终端在显示这个会话**（点 ↗ 时那台答一次）。
//!
//! - 不在 tmux 里：claude 进程自己的环境就是答案（它从那个终端的登录 shell 继承来）；进程已经没有控制终端 ⇒ `no-terminal`。
//! - 在 tmux 里：claude 的环境是建会话那个人的、可能早就断开了 ⇒ 不用它。问那个 socket（取自 claude 环境里的 `TMUX`）
//!   「现在有哪些客户端连着这个 pane 所在的会话」，逐个读那些客户端进程的环境，按最近动静倒序。一个都没有 ⇒ `detached`。
//!   tmux 在这里只被问「谁连着」，不读也不设标题。
//! - 读环境只抠写死的那几个键（`TMUX` · `TMUX_PANE` · `SSH_CONNECTION` · `LC_CCM_WINDOW`），不回整份环境。
//! - `LC_CCM_WINDOW` 是本机那个终端窗口的标签（PowerShell 接入块设、经 ssh 送过来），这里只原样回显，认窗口归本机。
//! - 零定时器：只在被问时答。读不了别人的进程 ⇒ 照实说 `unreadable`。

use crate::platform::child::{Child, Deadline};
use serde::Serialize;
use serde_json::Value;

use crate::platform::proc::{proc_env_var, EnvRead};

/// 本族的应答：成品或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

const TMUX_ENV: &str = "TMUX";
const TMUX_PANE_ENV: &str = "TMUX_PANE";
const SSH_CONNECTION_ENV: &str = "SSH_CONNECTION";
const WINDOW_LABEL_ENV: &str = "LC_CCM_WINDOW";
/// 一次最多报几个连着的终端（同一个会话连十几个客户端已经不正常；超出的按动静排在后面的丢掉）。
const MAX_TERMINALS: usize = 16;

/// `SSH_CONNECTION` 四元组（那台机器看到的：对面地址 · 对面端口 · 本机地址 · 本机端口）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SshConnection {
    pub(crate) client_addr: String,
    pub(crate) client_port: u16,
    pub(crate) server_addr: String,
    pub(crate) server_port: u16,
}

/// 一个正在显示这个会话的终端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Terminal {
    /// 经 ssh 连过来的那条连接；`None` = 那个终端不是经 ssh 连的（这台机器自己的屏幕上，或别的方式）。
    pub(crate) ssh: Option<SshConnection>,
    /// 最近一次动静（tmux 客户端的 `client_activity`，Unix 秒）；不在 tmux 里 ⇒ `None`。
    pub(crate) activity: Option<u64>,
    /// 本机那个终端窗口的标签（`LC_CCM_WINDOW`，形如 `<进程号>-<起始时刻>`）；没送过来 / 形状不对 ⇒ 不带。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) window: Option<String>,
}

/// 查到的事实：一串终端，或一条说得出的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// 最近动静在前。
    By(Vec<Terminal>),
    /// 在 tmux 里放在后台，现在没有终端连着。
    Detached,
    /// 不在 tmux 里，进程已经没有终端。
    NoTerminal,
    /// 读不了那个终端的信息（别的用户 / 权限 / 这一刻读不到）。
    Unreadable,
}

/// 帧面入口（宿主 `faces/feature_face.rs` 交家目录）：`{sid}` ⇒ `{terminals: [...]}` 或 `{terminals: [], why}`。
pub(crate) fn answer_at(home: &std::path::Path, args: &Value) -> Answer {
    let sid = args
        .get("sid")
        .and_then(Value::as_str)
        .filter(|s| shell_quote_core::session_id_ok(s))
        .ok_or((
            "bad_args",
            crate::common::contract::malformed("missing or malformed `sid`"),
        ))?;
    let pid = crate::observe::watcher::running_sessions(home)
        .into_iter()
        .find(|(s, _)| s == sid)
        .map(|(_, pid)| pid)
        .ok_or((
            "no_such_session",
            crate::common::contract::malformed(
                "no running session with that `sid` on this machine",
            ),
        ))?;
    crate::stream::inbound::spec::wire(&product(&shown_by(pid)?))
}

/// `session-terminals` 的应答：此刻显示它的终端（一个都没有 ⇒ 空 ＋ 为什么）。
#[derive(Serialize)]
pub(crate) struct Showing {
    pub(crate) terminals: Vec<Terminal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) why: Option<&'static str>,
}

/// 事实 ⇒ 线上成品。
pub(crate) fn product(shown: &Shown) -> Showing {
    let empty = |why| Showing {
        terminals: Vec::new(),
        why: Some(why),
    };
    match shown {
        Shown::By(t) => Showing {
            terminals: t.clone(),
            why: None,
        },
        Shown::Detached => empty("detached"),
        Shown::NoTerminal => empty("no-terminal"),
        Shown::Unreadable => empty("unreadable"),
    }
}

/// 读一个键：没设 / 读不到分开报（读不到 ⇒ 调用方说 `unreadable`，不当成「没有」）。
fn env(pid: u32, key: &str) -> Result<Option<String>, ()> {
    match proc_env_var(pid, key) {
        EnvRead::Value(v) => Ok(Some(v)),
        EnvRead::Unset => Ok(None),
        EnvRead::Unreadable => Err(()),
    }
}

/// `pid`（那个 claude 进程）此刻由谁在显示。tmux 起不来 / 报错 ⇒ `failed`（带它的原话）。
pub(crate) fn shown_by(pid: u32) -> Result<Shown, (&'static str, String)> {
    let Ok(tmux) = env(pid, TMUX_ENV) else {
        return Ok(Shown::Unreadable);
    };
    let Some(tmux) = tmux else {
        // 不在 tmux 里：它自己的环境就是那个终端的。
        return Ok(match crate::platform::proc::has_controlling_tty(pid) {
            Some(false) => Shown::NoTerminal,
            None => Shown::Unreadable,
            Some(true) => match terminal_of(pid, None) {
                Some(t) => Shown::By(vec![t]),
                None => Shown::Unreadable,
            },
        });
    };
    let socket = tmux.split(',').next().unwrap_or_default();
    let Ok(Some(pane)) = env(pid, TMUX_PANE_ENV) else {
        return Ok(Shown::Unreadable);
    };
    if !socket_ok(socket) || !pane_ok(pane.trim()) {
        return Ok(Shown::Unreadable);
    }
    let clients = list_clients(socket, pane.trim())?;
    if clients.is_empty() {
        return Ok(Shown::Detached);
    }
    let mut seen: Vec<Terminal> = clients
        .into_iter()
        .filter_map(|(cpid, activity)| terminal_of(cpid, Some(activity)))
        .collect();
    if seen.is_empty() {
        return Ok(Shown::Unreadable);
    }
    // 最近有动静的在前；同一秒的保持 tmux 列出的顺序（稳定排序）。
    seen.sort_by(|a, b| b.activity.cmp(&a.activity));
    seen.truncate(MAX_TERMINALS);
    Ok(Shown::By(seen))
}

/// 读一个终端进程的 `SSH_CONNECTION` 与窗口标签；环境读不到 ⇒ `None`（调用方据此说 `unreadable`）。
fn terminal_of(pid: u32, activity: Option<u64>) -> Option<Terminal> {
    let ssh = env(pid, SSH_CONNECTION_ENV).ok()?;
    let window = env(pid, WINDOW_LABEL_ENV).ok()?;
    Some(Terminal {
        ssh: ssh.as_deref().and_then(parse_ssh_connection),
        activity,
        window: window.as_deref().and_then(window_label),
    })
}

/// 窗口标签：`<十进制>-<十进制>`（进程号 · 起始时刻），总长有界；别的形状不回显。
pub(crate) fn window_label(raw: &str) -> Option<String> {
    let t = raw.trim();
    let (a, b) = t.split_once('-')?;
    let digits = |x: &str, max: usize| {
        !x.is_empty() && x.len() <= max && x.bytes().all(|c| c.is_ascii_digit())
    };
    (digits(a, 10) && digits(b, 20)).then(|| t.to_string())
}

/// `SSH_CONNECTION` 的四段：两个地址必须是 IP、两个口必须是端口号；不合 ⇒ `None`（当成不是经 ssh 连的）。
pub(crate) fn parse_ssh_connection(raw: &str) -> Option<SshConnection> {
    let f: Vec<&str> = raw.split_whitespace().collect();
    let [ca, cp, sa, sp] = f.as_slice() else {
        return None;
    };
    let addr = |a: &str| -> Option<String> {
        // 带作用域的 IPv6（`fe80::1%eth0`）：作用域只在那台机器上有意义，去掉再认。
        let bare = a.split('%').next().unwrap_or(a);
        bare.parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| ip.to_string())
    };
    Some(SshConnection {
        client_addr: addr(ca)?,
        client_port: cp.parse().ok()?,
        server_addr: addr(sa)?,
        server_port: sp.parse().ok()?,
    })
}

/// tmux socket 路径：绝对、不含控制字符（argv 直传不过 shell，挡的是读坏了的值）。
fn socket_ok(s: &str) -> bool {
    s.starts_with('/') && !s.chars().any(char::is_control)
}

/// pane id：`%` ＋ 十进制数字。
fn pane_ok(p: &str) -> bool {
    p.strip_prefix('%')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `list-clients` 那一发的期限：界面等 `session-terminals` 的预算是 15 s，tmux 一发 5 s（同 watcher 探测 tmux 的期限）。
const LIST_CLIENTS_WITHIN: Deadline = Deadline::secs(5);

/// 问那个 socket：连着 `pane` 所在会话的客户端（pid, 最近动静）。tmux 起不来 / 报错 ⇒ `failed`。
pub(crate) fn list_clients(
    socket: &str,
    pane: &str,
) -> Result<Vec<(u32, u64)>, (&'static str, String)> {
    let out = Child::new("tmux")
        .arg(crate::common::tmux_utf8::UTF8_CLIENT_FLAG)
        .args(["-S", socket, "list-clients", "-t", pane, "-F"])
        // 控制模式客户端（终端实时预览那个订阅者）整行留空 ⇒ 解析时丢掉：它没有终端窗口可切。
        .arg("#{?client_control_mode,,#{client_pid} #{client_activity}}")
        .run(LIST_CLIENTS_WITHIN)
        .map_err(|e| e.into_cmd_err("failed", |e| format!("tmux: {e}")))?;
    if !out.status.success() {
        return Err((
            "failed",
            format!(
                "tmux list-clients: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    Ok(parse_clients(&String::from_utf8_lossy(&out.stdout)))
}

/// `list-clients -F '#{client_pid} #{client_activity}'` 的输出 ⇒ (pid, 动静)。认不出的行丢掉。
pub(crate) fn parse_clients(raw: &str) -> Vec<(u32, u64)> {
    raw.lines()
        .filter_map(|l| {
            let (p, a) = l.trim().split_once(' ')?;
            Some((p.parse().ok()?, a.trim().parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/session_terminals_tests.rs"]
mod tests;
