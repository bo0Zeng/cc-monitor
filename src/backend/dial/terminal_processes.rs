//! 帧命令 `terminal-processes`：那台报来的「此刻显示这个会话的终端」（`session-terminals` 的 `terminals` 原样交来）
//! ⇒ **这台电脑上开着那条连接的进程，和它往上的进程链**；或一条说得出的原因。
//!
//! - 事实只来自一趟只读的系统查询（`platform::connection_and_process_tables`，直调系统接口：已建立的 TCP 连接表 ＋
//!   进程表的进程号 · 父进程号 · 名字 · 启动时刻四格）。不读任何进程的命令行、不读别的进程的内存。
//! - 判定全在这里：四元组全等认拥有者 · 往上数进程链（父进程比子进程晚起 ⇒ 那个进程号被复用过，链在那里断）·
//!   对不上时分「不在这台电脑上」与「这台电脑对不上」。窗口那一跳归 monitor。
//! - 几个终端按交来的顺序（最近动静在前）逐个试，第一个对上的就是它；都对不上 ⇒ 报第一个的原因。

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::IpAddr;

type CmdErr = (&'static str, String);

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
pub(crate) struct Link {
    pub(crate) pid: u32,
    pub(crate) name: String,
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

/// 帧面入口：现问系统那一趟。
pub(crate) fn answer(args: &Value) -> Result<Value, CmdErr> {
    answer_with(args, crate::platform::connection_and_process_tables)
}

/// [`answer`] 的本体，系统那一趟是参数（判据喂合成的 JSON）。入参先验，验不过不去问系统。
pub(crate) fn answer_with(
    args: &Value,
    query: impl FnOnce() -> Result<String, String>,
) -> Result<Value, CmdErr> {
    let wanted = read_args(args)?;
    let found = match query().and_then(|raw| parse_tables(&raw)) {
        Ok(t) => decide(&wanted, &t),
        Err(e) => {
            tracing::warn!("terminal-processes: 连接表 / 进程表没问成：{e}");
            Found::QueryFailed
        }
    };
    Ok(product(&found))
}

/// 事实 ⇒ 线上成品。
pub(crate) fn product(found: &Found) -> Value {
    match found {
        Found::Chain(c) => json!({ "chain": c }),
        Found::NotSsh => json!({ "chain": [], "why": "not-ssh" }),
        Found::Elsewhere(a) => json!({ "chain": [], "why": "elsewhere", "addr": a }),
        Found::Mismatch => json!({ "chain": [], "why": "mismatch" }),
        Found::QueryFailed => json!({ "chain": [], "why": "query-failed" }),
    }
}

fn bad(msg: &str) -> CmdErr {
    ("bad_args", crate::common::contract::malformed(msg))
}

/// 入参 `{terminals: [{ssh: {clientAddr, clientPort, serverAddr, serverPort} | null, …}…]}` ⇒ 每格要找的连接（`None` = 不是经 ssh）。
pub(crate) fn read_args(args: &Value) -> Result<Vec<Option<Wanted>>, CmdErr> {
    let ts = args
        .get("terminals")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty() && a.len() <= MAX_TERMINALS)
        .ok_or_else(|| bad("`terminals` must be a non-empty array of at most 16"))?;
    ts.iter()
        .map(|t| match t.get("ssh") {
            Some(Value::Null) => Ok(None),
            Some(s @ Value::Object(_)) => {
                let addr = |k: &str| s.get(k).and_then(Value::as_str).and_then(ip);
                let port = |k: &str| {
                    s.get(k)
                        .and_then(Value::as_u64)
                        .and_then(|p| u16::try_from(p).ok())
                };
                Ok(Some(Wanted {
                    client: addr("clientAddr").ok_or_else(|| bad("bad `clientAddr`"))?,
                    client_port: port("clientPort").ok_or_else(|| bad("bad `clientPort`"))?,
                    server: addr("serverAddr").ok_or_else(|| bad("bad `serverAddr`"))?,
                    server_port: port("serverPort").ok_or_else(|| bad("bad `serverPort`"))?,
                }))
            }
            _ => Err(bad("each terminal needs `ssh` (an object or null)")),
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
