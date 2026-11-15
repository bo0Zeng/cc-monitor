//! 帧命令 `session-terminals`：**此刻是哪个终端在显示这个会话**（点 ↗ 时那台答一次）。
//!
//! - 在托管终端里：claude 的环境是起后端那条 ssh 继承来的 ⇒ 不用它。问那个托管终端「此刻附着着哪些真终端」
//!   （`host::managed::terminal_ends_of_pid`），逐个读它们的环境，按最近动静倒序。一个都没有 ⇒ `detached`（能接回去）。
//!   只有那个托管终端前台的那一个才算（判法同 tmux 那一形，`host::tmux::seat::seat_with`，托管口认首领）：托管终端里再起的
//!   `claude -p`（没控制终端）⇒ `no-terminal`；后台进程组 · 别的终端上 · 说不清 ⇒ `unreadable`，不拿附着端顶。
//!   读不到终端那一样的平台（Windows · macOS）照 ccm 起的那一跳认前台（同打标那一判，`host::managed::terminal_ends_at`）：
//!   会话进程就是它（或它是「包一层」、会话进程是它的子进程）⇒ 报附着端；再往下的后代 ⇒ `unreadable`。
//! - 不在 tmux 里：claude 进程自己的环境就是答案（它从那个终端的登录 shell 继承来）；进程已经没有控制终端 ⇒ `no-terminal`。
//! - 在 tmux 里：claude 的环境是建会话那个人的、可能早就断开了 ⇒ 不用它。问那个 socket（取自 claude 环境里的 `TMUX`）
//!   「现在有哪些客户端连着这个 pane 所在的会话」，逐个读那些客户端进程的环境，按最近动静倒序。一个都没有 ⇒ `detached`。
//!   只有那个窗格前台的那一个才算（`host::tmux::seat::seat_of`：`TMUX_PANE` 往下继承，窗格里起的子进程、主会话里起的 `claude -p` 都带着它）：
//!   没控制终端 ⇒ `no-terminal`；有终端却不是那个窗格前台的（别的窗格 · 后台进程组）· 读不到 ⇒ `unreadable`，不拿窗格的客户端顶。
//!   tmux 在这里只被问「谁连着」，不读也不设标题。
//! - 读环境只抠写死的那几个键（`TMUX` · `TMUX_PANE` · `SSH_CONNECTION` · `LC_CCM_WINDOW`），不回整份环境。
//! - `LC_CCM_WINDOW` 是本机那个终端窗口的标签（PowerShell 接入块设、经 ssh 送过来），这里只原样回显，认窗口归本机。
//! - 零定时器：只在被问时答。读不了别人的进程 ⇒ 照实说 `unreadable`。
//! - 问它的是别台的本机后端（`--resident-call`，`faces/session_window_face.rs`）；本机会话那一问在同一进程里走 [`here_by`]（连进程号一起交）。

use crate::common::said::Said;
use serde::{Deserialize, Serialize};

use crate::platform::proc::{proc_env_var, EnvRead};

use crate::common::tmux_utf8::TMUX_ENV;
use crate::host::managed::Ends;
use crate::host::tmux::seat::{own_pane, seat_of, PaneRead, Seat};
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
    /// 进程已经没有控制终端（tmux 里外一样）。
    NoTerminal,
    /// 读不了那个终端的信息（别的用户 / 权限 / 这一刻读不到），或在 tmux 里却不是那个窗格前台的那一个（说不清谁在显示它）。
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
pub fn answer_at(home: impl crate::agents::IntoHomes, req: &SidReq) -> Result<Showing, Said> {
    let home = home.into_homes();
    let sid = req.sid.as_str();
    let pid = crate::history::live_table::now(&home)
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
    /// 进程没有控制终端。
    NoTerminal,
    /// 读不了 · 说不清。
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
    shown_in(crate::host::managed::terminal_ends_of_pid(pid), pid)
}

/// [`shown_by`] 的本体：`hosted` 是托管那一问的答案（`host::managed::terminal_ends_at`；不住在托管终端里 ⇒ `None`）。
pub(crate) fn shown_in(hosted: Option<Ends>, pid: u32) -> Result<Shown, Said> {
    // 住在托管终端里：显示它的是附着上来的真终端；claude 自己的环境是起后端那条 ssh 继承来的，不是谁在看它。
    // 只有那个托管终端前台的那一个才由它们显示（判法同 tmux 窗格那一处，`seat_with`）：托管终端里再起的 `claude -p`
    // （没控制终端）⇒ `no-terminal`；后台进程组 · 别的终端上 · 说不清 ⇒ `unreadable`（非 Linux 上托管口照旧交附着端）。
    match hosted {
        Some(Ends::Front(ends)) => return Ok(hosted_shown(ends, |p, at| terminal_of(p, Some(at)))),
        Some(Ends::Not(Seat::Detached)) => return Ok(Shown::NoTerminal),
        Some(Ends::Not(_)) => return Ok(Shown::Unreadable),
        None => {}
    }
    let Ok(tmux) = env(pid, TMUX_ENV) else {
        return Ok(Shown::Unreadable);
    };
    let Some(tmux) = tmux else {
        // 不在 tmux 里：它自己的环境就是那个终端的。
        return Ok(match seat_of(pid, None) {
            Seat::Detached => Shown::NoTerminal,
            Seat::OnPane => match terminal_of(pid, None) {
                Some(t) => Shown::By {
                    terminals: vec![t],
                    tmux: false,
                },
                None => Shown::Unreadable,
            },
            Seat::Elsewhere | Seat::Unknown => Shown::Unreadable,
        });
    };
    // 在 tmux 里：只有那个窗格前台的那一个才由连着那个会话的客户端显示（环境里的 `TMUX_PANE` 往下继承，子进程也带着）。
    let pane = match on_pane(pid) {
        Ok(pane) => pane,
        Err(Seat::Detached) => return Ok(Shown::NoTerminal),
        Err(_) => return Ok(Shown::Unreadable),
    };
    Ok(match tmux_clients(&pane, &tmux)? {
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

/// `pid`（本机那个 claude 进程）此刻由哪个终端进程显示：托管终端前台的那一个 ⇒ 附着着的那几扇（一个都没有 ⇒ `Detached`）；
/// 在 tmux 里 ⇒ 连着它的 tmux 客户端（一个都没有 ⇒ `Detached`）；托管终端里不是前台的那一个、不在 tmux 里、读不出 tmux 那几格（Windows 读不到别的进程的环境）、或不是那个窗格前台的那一个 ⇒ 它自己（它挂在那个终端上）；
/// 没控制终端 · 别的窗格 · 后台的不说在 tmux 里。
pub fn here_by(pid: u32) -> Result<Here, Said> {
    here_in(crate::host::managed::terminal_ends_of_pid(pid), pid)
}

/// [`here_by`] 的本体：`hosted` 同 [`shown_in`]。
pub(crate) fn here_in(hosted: Option<Ends>, pid: u32) -> Result<Here, Said> {
    // 住在托管终端里：显示它的是附着上来的那几扇（同 [`shown_in`]）；claude 自己挂在托管口的伪终端上，往上没有窗口。
    // 不是那个托管终端前台的那一个（没控制终端 · 后台的 · 说不清）⇒ 那几扇不在显示它，链从它自己起、不说在 tmux 里。
    match hosted {
        Some(Ends::Front(ends)) => return hosted_here(ends, |p, at| terminal_of(p, Some(at))),
        Some(Ends::Not(_)) => return Ok(itself(pid, false)),
        None => {}
    }
    let tmux_env = env(pid, TMUX_ENV);
    let mut in_tmux = matches!(tmux_env, Ok(Some(_)));
    if let Ok(Some(tmux)) = &tmux_env {
        match on_pane(pid) {
            Ok(pane) => match tmux_clients(&pane, tmux)? {
                Clients::By(seen) => return Ok(Here::By { seen, tmux: true }),
                Clients::Detached => return Ok(Here::Detached),
                Clients::Unreadable => {}
            },
            // 不是那个窗格前台的那一个（没控制终端 · 别的窗格 · 后台的）：那个窗格不在显示它，接回那个 tmux 会话也看不到它。
            Err(Seat::Detached | Seat::Elsewhere) => in_tmux = false,
            Err(Seat::OnPane | Seat::Unknown) => {}
        }
    }
    Ok(itself(pid, in_tmux))
}

/// 链从 `pid` 自己起（它挂在那个终端上；读不出它的终端那几格 ⇒ 空着）。
fn itself(pid: u32, tmux: bool) -> Here {
    let own = terminal_of(pid, None).unwrap_or(Terminal {
        ssh: None,
        activity: None,
        window: None,
    });
    Here::By {
        seen: vec![(pid, own)],
        tmux,
    }
}

/// 在 tmux 里的 `pid` 是不是它 `TMUX_PANE` 那个窗格前台的那一个（判法只此一处 `host::tmux::seat::own_pane`）：
/// 是 ⇒ 那个窗格；不是 ⇒ 它跟那个窗格的关系；`TMUX_PANE` 说不出 ⇒ 没控制终端的 [`Seat::Detached`]，别的 [`Seat::Unknown`]。
fn on_pane(pid: u32) -> Result<String, Seat> {
    match own_pane(pid) {
        (PaneRead::Pane(pane), Seat::OnPane) => Ok(pane),
        (PaneRead::Pane(_), seat) => Err(seat),
        (PaneRead::NotSet | PaneRead::Unknown, Seat::Detached) => Err(Seat::Detached),
        (PaneRead::NotSet | PaneRead::Unknown, _) => Err(Seat::Unknown),
    }
}

/// 本机那一问里托管终端的会话由谁显示：附着上来的那几扇（`(进程号, 最近动静秒)`，最近动静在前）连进程号一起交，链从它们起；
/// 读不出那个终端的环境（Windows）⇒ 照样交进程号、终端那几格空着（壳沿链找窗口）；交不出进程号的不算。一个都没有 ⇒ [`Here::Detached`]；
/// 问托管口没问成 ⇒ `failed`。**纯函数**（读法是参数）。
pub(crate) fn hosted_here(
    ends: std::io::Result<Vec<(Option<u32>, u64)>>,
    read: impl Fn(u32, u64) -> Option<Terminal>,
) -> Result<Here, Said> {
    let ends = ends.map_err(|e| Said::new("failed", format!("managed terminal: {e}")))?;
    let seen: Vec<(u32, Terminal)> = ends
        .into_iter()
        .filter_map(|(pid, at)| {
            let pid = pid?;
            Some((
                pid,
                read(pid, at).unwrap_or(Terminal {
                    ssh: None,
                    activity: Some(at),
                    window: None,
                }),
            ))
        })
        .take(MAX_TERMINALS)
        .collect();
    Ok(if seen.is_empty() {
        Here::Detached
    } else {
        Here::By { seen, tmux: false }
    })
}

/// 问 tmux 谁连着：每个客户端的进程号与终端（最近动静在前）· 没有 · 读不出。
enum Clients {
    By(Vec<(u32, Terminal)>),
    Detached,
    Unreadable,
}

/// `pane`（核过形状的窗格）与 `TMUX`（`tmux`）⇒ 那个 socket 上连着那个窗格所在会话的客户端。tmux 起不来 / 报错 ⇒ `failed`。
fn tmux_clients(pane: &str, tmux: &str) -> Result<Clients, Said> {
    let socket = crate::host::tmux::socket_of(tmux).unwrap_or_default();
    if !socket_ok(socket) {
        return Ok(Clients::Unreadable);
    }
    let clients = list_clients(socket, pane)?;
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

#[cfg(test)]
#[path = "../../../tests/backend/observe/session_terminals_seat_tests.rs"]
mod seat_tests;
