//! ↗ 那一问（`session-window`，`faces/session_window_face.rs`）的本机那一半：那台报来的「此刻显示这个会话的终端」
//! （那台 `session-terminals` 的 `terminals`）⇒ **这台电脑上开着那条连接的进程，和它往上的进程链**；或一条说得出的原因。
//! 本机会话的链也从这里数（[`chain`]：从显示它的那个终端进程往上）。
//!
//! - 事实只来自一趟只读的系统查询（`platform::connection_and_process_tables`，直调系统接口：已建立的 TCP 连接表 ＋
//!   进程表的进程号 · 父进程号 · 名字 · 启动时刻四格）。不读任何进程的命令行、不读别的进程的内存。
//! - 判定全在这里：四元组全等认拥有者 · 往上数进程链（父进程比子进程晚起 ⇒ 那个进程号被复用过，链在那里断）·
//!   对不上时分「不在这台电脑上」与「这台电脑对不上」。窗口那一跳归 monitor。
//! - 几个终端按交来的顺序（最近动静在前）逐个试，第一个对上的就是它；都对不上 ⇒ 报第一个的原因。

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::net::IpAddr;

use crate::common::said::Said;

/// 一次最多看几个终端（与那台一次最多报几个同值）。
const MAX_TERMINALS: usize = 16;
/// 进程链最多几级（父进程表成环 / 读坏了也停得下来）。
const MAX_CHAIN: usize = 32;

/// 那台看到的那条连接（`SSH_CONNECTION` 四段）：对面 ＝ 这台电脑这一端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub(crate) client: IpAddr,
    pub(crate) client_port: u16,
    pub(crate) server: IpAddr,
    pub(crate) server_port: u16,
}

/// 连接表一行（已建立）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conn {
    pub(crate) local: IpAddr,
    pub(crate) local_port: u16,
    pub(crate) remote: IpAddr,
    pub(crate) remote_port: u16,
    pub(crate) pid: u32,
}

/// 进程表一行（只这四格）。`start` 是启动时刻（只在同一台上比先后：Windows FILETIME · Linux 开机后的滴答）；0 = 系统没给。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Proc {
    pub(crate) ppid: u32,
    pub(crate) name: String,
    pub(crate) start: u64,
}

/// 两张表。
#[derive(Debug, Default)]
pub(crate) struct Tables {
    pub(crate) conns: Vec<Conn>,
    pub(crate) procs: HashMap<u32, Proc>,
}

/// 进程链的一级（线上形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct Link {
    /// 进程号。
    pub(crate) pid: u32,
    /// 进程名。
    pub(crate) name: String,
    /// 启动时刻（系统原值；拿不到 ⇒ 0）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) start: u64,
}

/// 查到的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Found {
    /// 开着那条连接的进程在前，往上到桌面外壳 / init 之前。
    Chain(Vec<Link>),
    /// 那个终端不是经 ssh 连的（那台报的是 `ssh: null`）。
    NotSsh,
    /// 那条连接不在这台电脑上：这台既没有那个地址，也没有 ssh 连着它（带那台看到的对面地址）。
    Elsewhere(String),
    /// 这台电脑有那个地址、或有 ssh 连着那个地址，但四元组对不上（经跳板机 / 端口转换）。
    Mismatch,
    /// 这一次没查成（系统那一趟报错 · 两张表对不上）。
    QueryFailed,
}

/// 要对的那几个终端：那台 `session-terminals` 答的 `terminals`，原样交来。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalProcessesReq {
    /// 1–16 个，最近动静在前。
    pub(crate) terminals: Vec<crate::common::seen_terminal::Terminal>,
}

/// 本体，系统那一趟是参数（判据喂合成的 JSON）。入参先验，验不过不去问系统（码只有 `bad_args`）。
pub(crate) fn answer_with(
    req: &TerminalProcessesReq,
    query: impl FnOnce() -> Result<String, String>,
) -> Result<Processes, Said> {
    let wanted = read_args(req)?;
    let found = match query().and_then(|raw| parse_tables(&raw)) {
        Ok(t) => decide(&wanted, &t),
        Err(e) => {
            tracing::warn!("session-window: 连接表 / 进程表没问成：{e}");
            Found::QueryFailed
        }
    };
    Ok(product(&found))
}

/// 对出来的：进程链（对不上 ⇒ 空链 ＋ 为什么，`elsewhere` 另带对面地址）。`session-window` 的回包照搬这三格。
#[derive(Debug, Serialize)]
pub(crate) struct Processes {
    /// 这台上开着那条连接的进程链（自下而上）。
    pub(crate) chain: Vec<Link>,
    /// 对不上时：`not-ssh` · `elsewhere`（不是这台开的）· `mismatch`（经跳板 / 端口转换）· `query-failed`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) why: Option<&'static str>,
    /// `elsewhere` 时对面那个地址。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) addr: Option<String>,
}

/// 事实 ⇒ 线上成品。
pub(crate) fn product(found: &Found) -> Processes {
    let empty = |why| Processes {
        chain: Vec::new(),
        why: Some(why),
        addr: None,
    };
    match found {
        Found::Chain(c) => Processes {
            chain: c.clone(),
            why: None,
            addr: None,
        },
        Found::NotSsh => empty("not-ssh"),
        Found::Elsewhere(a) => Processes {
            addr: Some(a.clone()),
            ..empty("elsewhere")
        },
        Found::Mismatch => empty("mismatch"),
        Found::QueryFailed => empty("query-failed"),
    }
}

fn bad(msg: &str) -> Said {
    ("bad_args", crate::common::contract::malformed(msg))
}

/// 入参 ⇒ 每格要找的连接（`None` = 不是经 ssh）。个数 1–16；地址认不出 ⇒ `bad_args`。
pub(crate) fn read_args(req: &TerminalProcessesReq) -> Result<Vec<Option<Wanted>>, Said> {
    let ts = &req.terminals;
    if ts.is_empty() || ts.len() > MAX_TERMINALS {
        return Err(bad("`terminals` must be a non-empty array of at most 16"));
    }
    ts.iter()
        .map(|t| match &t.ssh {
            None => Ok(None),
            Some(s) => Ok(Some(Wanted {
                client: ip(&s.client_addr).ok_or_else(|| bad("bad `clientAddr`"))?,
                client_port: s.client_port,
                server: ip(&s.server_addr).ok_or_else(|| bad("bad `serverAddr`"))?,
                server_port: s.server_port,
            })),
        })
        .collect()
}

/// 一个地址：去掉作用域（`%…`），IPv4 映射的 IPv6 认成 IPv4（两边写法不同也是同一个地址）。
pub(crate) fn ip(s: &str) -> Option<IpAddr> {
    let bare = s.split('%').next().unwrap_or(s);
    let a: IpAddr = bare.parse().ok()?;
    Some(match a {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(a, IpAddr::V4),
        v4 => v4,
    })
}

/// 系统那一趟的 JSON ⇒ 两张表。认不出的行丢掉；整份认不出 ⇒ `Err`。
pub(crate) fn parse_tables(raw: &str) -> Result<Tables, String> {
    let v: Value = serde_json::from_str(raw.trim()).map_err(|e| format!("not JSON: {e}"))?;
    // PowerShell 把只有一项的数组打成单个对象 —— 两形都收。
    let rows = |k: &str| -> Result<Vec<Value>, String> {
        match v.get(k) {
            Some(Value::Array(a)) => Ok(a.clone()),
            Some(o @ Value::Object(_)) => Ok(vec![o.clone()]),
            Some(Value::Null) => Ok(Vec::new()),
            _ => Err(format!("missing `{k}`")),
        }
    };
    let num = |r: &Value, k: &str| r.get(k).and_then(Value::as_u64);
    let conns = rows("tcp")?
        .iter()
        .filter_map(|r| {
            Some(Conn {
                local: ip(r.get("la")?.as_str()?)?,
                local_port: u16::try_from(num(r, "lp")?).ok()?,
                remote: ip(r.get("ra")?.as_str()?)?,
                remote_port: u16::try_from(num(r, "rp")?).ok()?,
                pid: u32::try_from(num(r, "pid")?).ok()?,
            })
        })
        .collect();
    let procs = rows("proc")?
        .iter()
        .filter_map(|r| {
            Some((
                u32::try_from(num(r, "pid")?).ok()?,
                Proc {
                    ppid: num(r, "ppid")
                        .and_then(|p| u32::try_from(p).ok())
                        .unwrap_or(0),
                    name: r
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    start: num(r, "start").unwrap_or(0),
                },
            ))
        })
        .collect();
    Ok(Tables { conns, procs })
}

/// 逐个终端试：第一个对上的给进程链；都对不上 ⇒ 第一个的原因。
pub(crate) fn decide(wanted: &[Option<Wanted>], t: &Tables) -> Found {
    let mut first: Option<Found> = None;
    for w in wanted {
        let f = match w {
            None => Found::NotSsh,
            Some(w) => one(w, t),
        };
        if matches!(f, Found::Chain(_)) {
            return f;
        }
        first.get_or_insert(f);
    }
    first.unwrap_or(Found::QueryFailed)
}

/// 一条连接：四元组全等 ⇒ 拥有者往上的进程链；对不上 ⇒ 分两种原因（只看这台的连接表，不猜）。
fn one(w: &Wanted, t: &Tables) -> Found {
    let hit = t.conns.iter().find(|c| {
        c.local == w.client
            && c.local_port == w.client_port
            && c.remote == w.server
            && c.remote_port == w.server_port
    });
    if let Some(c) = hit {
        return match chain(c.pid, &t.procs) {
            Some(ch) => Found::Chain(ch),
            // 连接表里有它、进程表里没有：两次读之间那个进程退了。
            None => Found::QueryFailed,
        };
    }
    // 这台有那个地址（端口被改了），或这台有 ssh 连着那个地址（那台看到的是跳板）⇒ 这台电脑对不上。
    let here = t.conns.iter().any(|c| {
        c.local == w.client
            || (c.remote == w.client
                && t.procs
                    .get(&c.pid)
                    .is_some_and(|p| named(&p.name, crate::platform::SSH_CLIENT_NAMES)))
    });
    if here {
        Found::Mismatch
    } else {
        Found::Elsewhere(w.client.to_string())
    }
}

/// 本机开过的那个终端（`dial/opened_windows.rs` 记下的标签 `<shell 进程号>-<起始时刻>`）此刻还开着 ssh 吗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenedHere {
    /// 那个 shell 还是它，它自己就是 ssh（shell 把 ssh exec 了：进程号与起始时刻不变）或底下开着 ssh ⇒ 从那个 ssh 往上的进程链。
    Chain(Vec<Link>),
    /// 那个 shell 还在，底下没有 ssh（ssh 退了、窗口还开着）。
    NoSsh,
    /// 那个进程号不在、或起始时刻对不上（shell 退了、进程号被复用了）。
    Gone,
}

/// 标签那个 shell 在这台进程表里的样子（[`OpenedHere`]）。底下几个 ssh ⇒ 最早起的那个（前奏之后起的那一个）。
pub(crate) fn opened_chain(
    label: &crate::common::seen_terminal::WindowLabel,
    procs: &HashMap<u32, Proc>,
) -> OpenedHere {
    let (pid, start) = (label.pid, label.start);
    let Some(shell) = procs.get(&pid).filter(|p| p.start != 0 && p.start == start) else {
        return OpenedHere::Gone;
    };
    let ssh = |p: &Proc| named(&p.name, crate::platform::SSH_CLIENT_NAMES);
    let at = if ssh(shell) {
        Some(pid)
    } else {
        procs
            .iter()
            .filter(|(c, p)| **c != pid && p.ppid == pid && ssh(p) && p.start >= start)
            .min_by_key(|(c, p)| (p.start, **c))
            .map(|(c, _)| *c)
    };
    match at.and_then(|at| chain(at, procs)) {
        Some(c) => OpenedHere::Chain(c),
        None => OpenedHere::NoSsh,
    }
}

/// 进程名是不是这几个之一（不分大小写：Windows 的名字大小写不定）。
fn named(name: &str, names: &[&str]) -> bool {
    names.iter().any(|n| name.eq_ignore_ascii_case(n))
}

/// 从 `pid` 往上数：到桌面外壳 / init 之前为止；父进程不在表里 / 比子进程晚起（进程号被复用过）/ 启动时刻不明 ⇒ 链在那里断。
/// `pid` 自己不在进程表里 ⇒ `None`。
pub(crate) fn chain(pid: u32, procs: &HashMap<u32, Proc>) -> Option<Vec<Link>> {
    let mut cur = pid;
    let mut p = procs.get(&cur)?;
    let mut out = vec![Link {
        pid: cur,
        name: p.name.clone(),
        start: p.start,
    }];
    while out.len() < MAX_CHAIN {
        let Some(parent) = procs.get(&p.ppid).filter(|_| p.ppid != 0 && p.ppid != cur) else {
            break;
        };
        if parent.start == 0 || p.start == 0 || parent.start > p.start {
            break;
        }
        if named(&parent.name, crate::platform::CHAIN_STOP_NAMES)
            || out.iter().any(|l| l.pid == p.ppid)
        {
            break;
        }
        cur = p.ppid;
        p = parent;
        out.push(Link {
            pid: cur,
            name: p.name.clone(),
            start: p.start,
        });
    }
    Some(out)
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_terminal_processes_tests.rs"]
mod tests;
