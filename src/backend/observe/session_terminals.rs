//! 帧命令 `session-terminals`：**此刻是哪个终端在显示这个会话**（点 ↗ 时那台答一次）。
//!
//! - 在托管终端里：claude 的环境是起后端那条 ssh 继承来的 ⇒ 不用它。问那个托管终端「此刻附着着哪些真终端」
//!   （`host::managed::terminal_ends_of_pid`），逐个读它们的环境，按最近动静倒序。一个都没有 ⇒ `detached`（能接回去）。
//! - 不在 tmux 里：claude 进程自己的环境就是答案（它从那个终端的登录 shell 继承来）；进程已经没有控制终端 ⇒ `no-terminal`。
//! - 在 tmux 里：claude 的环境是建会话那个人的、可能早就断开了 ⇒ 不用它。问那个 socket（取自 claude 环境里的 `TMUX`）
//!   「现在有哪些客户端连着这个 pane 所在的会话」，逐个读那些客户端进程的环境，按最近动静倒序。一个都没有 ⇒ `detached`。
//!   tmux 在这里只被问「谁连着」，不读也不设标题。
//! - 读环境只抠写死的那几个键（`TMUX` · `TMUX_PANE` · `SSH_CONNECTION` · `LC_CCM_WINDOW`），不回整份环境。
//! - `LC_CCM_WINDOW` 是本机那个终端窗口的标签（PowerShell 接入块设、经 ssh 送过来），这里只原样回显，认窗口归本机。
//! - 零定时器：只在被问时答。读不了别人的进程 ⇒ 照实说 `unreadable`。
//! - 问它的是别台的本机后端（`--resident-call`，`faces/session_window_face.rs`）；本机会话那一问在同一进程里走 [`here_by`]（连进程号一起交）。

use crate::common::said::Said;
use serde::{Deserialize, Serialize};

use crate::platform::proc::{proc_env_var, EnvRead};

const TMUX_ENV: &str = "TMUX";
const TMUX_PANE_ENV: &str = "TMUX_PANE";
const SSH_CONNECTION_ENV: &str = "SSH_CONNECTION";
const WINDOW_LABEL_ENV: &str = "LC_CCM_WINDOW";
/// 一次最多报几个连着的终端（同一个会话连十几个客户端已经不正常；超出的按动静排在后面的丢掉）。
const MAX_TERMINALS: usize = 16;

pub use crate::common::seen_terminal::{SshConnection, Terminal};

/// 查到的事实：一串终端，或一条说得出的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shown {
    /// 最近动静在前；`tmux`：会话在 tmux 里（这一串是连着它的 tmux 客户端）。
    By {
        terminals: Vec<Terminal>,
        tmux: bool,
    },
    /// 在 tmux 里放在后台，现在没有终端连着。
    Detached,
    /// 在托管终端里，现在没有真终端附着（能接回去，同 [`Shown::Detached`]，只是不在 tmux 里）。
    HostedDetached,
    /// 不在 tmux 里，进程已经没有终端。
    NoTerminal,
    /// 读不了那个终端的信息（别的用户 / 权限 / 这一刻读不到）。
    Unreadable,
}

/// `session-terminals` 的入参。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SidReq {
    /// 会话 id。
    pub sid: cmd_wire::Sid,
}

/// 帧面入口（宿主 `faces/feature_face.rs` 交家目录）：此刻显示它的终端，或一条原因。
/// 码：`bad_args`（sid 形状不对）· `no_such_session` · `failed` / `child_timed_out`（问 tmux 那一发）。
pub fn answer_at(home: &std::path::Path, req: &SidReq) -> Result<Showing, Said> {
    let sid = req.sid.as_str();
    let pid = crate::history::live_table::now(home)
        .writers(sid)
        .first()
        .copied()
        .ok_or_else(|| {
            Said::new(
                "no_such_session",
                crate::common::contract::malformed(
                    "no running session with that `sid` on this machine",
                ),
            )
        })?;
    Ok(product(&shown_by(pid)?))
}

/// `session-terminals` 的应答：此刻显示它的终端（一个都没有 ⇒ 空 ＋ 为什么）。
/// 问它的那台本机后端（`session-window`）照同一个类型解回来。
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Showing {
    /// 此刻显示它的终端，最近动静在前，最多 16 个。
    pub terminals: Vec<Terminal>,
    /// 一个都没有时为什么。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<ShownWhy>,
    /// 会话在 tmux 里（问它的那台照带给出口：窗口关了时给不给［在终端里打开］）。旧版那台不带 ⇒ 当不在。
    #[serde(default)]
    pub tmux: bool,
}

/// 一个终端都没有的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "kebab-case")]
pub enum ShownWhy {
    /// 在 tmux 或托管终端里、没终端连着（能接回去：本机照它给［在终端里打开］）。
    Detached,
    /// 不在 tmux 里、进程没有终端。
    NoTerminal,
    /// 读不了。
    Unreadable,
}

/// 事实 ⇒ 线上成品。
pub(crate) fn product(shown: &Shown) -> Showing {
    let empty = |why| Showing {
        terminals: Vec::new(),
        why: Some(why),
        tmux: false,
    };
    match shown {
        Shown::By { terminals, tmux } => Showing {
            terminals: terminals.clone(),
            why: None,
            tmux: *tmux,
        },
        Shown::Detached => Showing {
            tmux: true,
            ..empty(ShownWhy::Detached)
        },
        Shown::HostedDetached => empty(ShownWhy::Detached),
        Shown::NoTerminal => empty(ShownWhy::NoTerminal),
        Shown::Unreadable => empty(ShownWhy::Unreadable),
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
pub(crate) fn shown_by(pid: u32) -> Result<Shown, Said> {
    // 住在托管终端里：显示它的是附着上来的真终端；claude 自己的环境是起后端那条 ssh 继承来的，不是谁在看它。
    if let Some(ends) = crate::host::managed::terminal_ends_of_pid(pid) {
        return Ok(hosted_shown(ends, |p, at| terminal_of(p, Some(at))));
    }
    let Ok(tmux) = env(pid, TMUX_ENV) else {
        return Ok(Shown::Unreadable);
    };
    let Some(tmux) = tmux else {
        // 不在 tmux 里：它自己的环境就是那个终端的。
        return Ok(match crate::platform::proc::has_controlling_tty(pid) {
            Some(false) => Shown::NoTerminal,
            None => Shown::Unreadable,
            Some(true) => match terminal_of(pid, None) {
                Some(t) => Shown::By {
                    terminals: vec![t],
                    tmux: false,
                },
                None => Shown::Unreadable,
            },
        });
    };
    Ok(match tmux_clients(pid, &tmux)? {
        Clients::By(seen) => Shown::By {
            terminals: seen.into_iter().map(|(_, t)| t).collect(),
            tmux: true,
        },
        Clients::Detached => Shown::Detached,
        Clients::Unreadable => Shown::Unreadable,
    })
}

/// 托管终端里的会话由谁显示：附着上来的真终端（`(进程号, 最近动静秒)`，最近动静在前；`read` 读一个终端进程的那几格）。
/// 一个都没有 ⇒ [`Shown::HostedDetached`]；问托管口没问成 · 一个都读不出 ⇒ [`Shown::Unreadable`]。**纯函数**（读法是参数）。
pub(crate) fn hosted_shown(
    ends: std::io::Result<Vec<(Option<u32>, u64)>>,
    read: impl Fn(u32, u64) -> Option<Terminal>,
) -> Shown {
    let Ok(ends) = ends else {
        return Shown::Unreadable;
    };
    if ends.is_empty() {
        return Shown::HostedDetached;
    }
    let terminals: Vec<Terminal> = ends
        .into_iter()
        .filter_map(|(pid, at)| read(pid?, at))
        .take(MAX_TERMINALS)
        .collect();
    if terminals.is_empty() {
        Shown::Unreadable
    } else {
        Shown::By {
            terminals,
            tmux: false,
        }
    }
}

/// 本机那一问（`session-window`）要的：此刻显示这个会话的终端进程（链从它起）和它的终端，最近动静在前；或没有终端连着。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Here {
    /// `tmux`：会话在 tmux 里（它的环境里有 `TMUX`）。
    By {
        seen: Vec<(u32, Terminal)>,
        tmux: bool,
    },
    Detached,
}

/// `pid`（本机那个 claude 进程）此刻由哪个终端进程显示：在 tmux 里 ⇒ 连着它的 tmux 客户端（一个都没有 ⇒ `Detached`）；
/// 不在 tmux 里、或读不出 tmux 那几格（Windows 读不到别的进程的环境）⇒ 它自己（它挂在那个终端上）。
pub fn here_by(pid: u32) -> Result<Here, Said> {
    let in_tmux = env(pid, TMUX_ENV);
    if let Ok(Some(tmux)) = &in_tmux {
        match tmux_clients(pid, tmux)? {
            Clients::By(seen) => return Ok(Here::By { seen, tmux: true }),
            Clients::Detached => return Ok(Here::Detached),
            Clients::Unreadable => {}
        }
    }
    let own = terminal_of(pid, None).unwrap_or(Terminal {
        ssh: None,
        activity: None,
        window: None,
    });
    Ok(Here::By {
        seen: vec![(pid, own)],
        tmux: matches!(in_tmux, Ok(Some(_))),
    })
}

/// 问 tmux 谁连着：每个客户端的进程号与终端（最近动静在前）· 没有 · 读不出。
enum Clients {
    By(Vec<(u32, Terminal)>),
    Detached,
    Unreadable,
}

/// `pid` 的 `TMUX`（`tmux`）⇒ 那个 socket 上连着它所在会话的客户端。tmux 起不来 / 报错 ⇒ `failed`。
fn tmux_clients(pid: u32, tmux: &str) -> Result<Clients, Said> {
    let socket = tmux.split(',').next().unwrap_or_default();
    let Ok(Some(pane)) = env(pid, TMUX_PANE_ENV) else {
        return Ok(Clients::Unreadable);
    };
    if !socket_ok(socket) || !pane_ok(pane.trim()) {
        return Ok(Clients::Unreadable);
    }
    let clients = list_clients(socket, pane.trim())?;
    if clients.is_empty() {
        return Ok(Clients::Detached);
    }
    let mut seen: Vec<(u32, Terminal)> = clients
        .into_iter()
        .filter_map(|(cpid, activity)| Some((cpid, terminal_of(cpid, Some(activity))?)))
        .collect();
    if seen.is_empty() {
        return Ok(Clients::Unreadable);
    }
    // 最近有动静的在前；同一秒的保持 tmux 列出的顺序（稳定排序）。
    seen.sort_by_key(|c| std::cmp::Reverse(c.1.activity));
    seen.truncate(MAX_TERMINALS);
    Ok(Clients::By(seen))
}

/// 读一个终端进程的 `SSH_CONNECTION` 与窗口标签；环境读不到 ⇒ `None`（调用方据此说 `unreadable`）。
fn terminal_of(pid: u32, activity: Option<u64>) -> Option<Terminal> {
    let ssh = env(pid, SSH_CONNECTION_ENV).ok()?;
    let window = env(pid, WINDOW_LABEL_ENV).ok()?;
    Some(Terminal {
        ssh: ssh.as_deref().and_then(parse_ssh_connection),
        activity,
        window: window.and_then(|w| w.parse().ok()),
    })
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

/// 问那个 socket：连着 `pane` 所在会话的客户端（pid, 最近动静）。tmux 起不来 / 报错 ⇒ `failed`。
pub fn list_clients(socket: &str, pane: &str) -> Result<Vec<(u32, u64)>, Said> {
    #[expect(
        clippy::disallowed_methods,
        reason = "点 ↗ 时答此刻谁在显示这个会话：只读 tmux，只在被问时起一次"
    )]
    let out = crate::host::tmux::child(None)
        .arg(crate::common::tmux_utf8::UTF8_CLIENT_FLAG)
        .args(["-S", socket, "list-clients", "-t", pane, "-F"])
        // 控制模式客户端（终端实时预览那个订阅者）整行留空 ⇒ 解析时丢掉：它没有终端窗口可切。
        .arg("#{?client_control_mode,,#{client_pid} #{client_activity}}")
        .run(crate::host::tmux::TMUX_WITHIN)
        .map_err(|e| e.into_cmd_err("failed", |e| format!("tmux: {e}")))?;
    if !out.status.success() {
        return Err(Said::new(
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
