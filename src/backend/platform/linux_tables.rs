//! **这台 Linux 的已建立 TCP 连接表 ＋ 进程表**（↗ 那一问的系统事实，`dial/terminal_processes.rs` 判；Windows 那份是 `win_tables.rs`）——
//! 只读 `/proc`：`/proc/net/tcp` · `/proc/net/tcp6`（已建立的那几行，带 socket 的 inode）· `/proc/<pid>/fd/*` 的链接
//! （`socket:[inode]` ⇒ 拥有者进程号；读不了的进程就跳过 —— 只有本用户的进程看得到，ssh 恰好是本用户开的）·
//! `/proc/<pid>/stat`（父进程号 · 名字 · 启动时刻）。不读任何进程的命令行、环境与内存。
//!
//! 产出与 Windows 那份同形的一行 JSON（`{tcp:[{la,lp,ra,rp,pid}…], proc:[{pid,ppid,name,start}…]}`，
//! `start` 是开机后的时钟滴答），解析与判定照旧只在 `terminal_processes.rs`。

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// 一族地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    V4,
    V6,
}

/// `/proc/net/tcp*` 里已建立的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TcpLine {
    pub(crate) local: IpAddr,
    pub(crate) local_port: u16,
    pub(crate) remote: IpAddr,
    pub(crate) remote_port: u16,
    pub(crate) inode: u64,
}

/// `/proc/<pid>/stat` 里要的三格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatLine {
    pub(crate) ppid: u32,
    pub(crate) name: String,
    pub(crate) start: u64,
}

/// `TCP_ESTABLISHED`（内核打成两位十六进制）。
const ESTABLISHED: &str = "01";

/// 内核把地址按「每 32 位一个本机字节序的整数」打成十六进制 ⇒ 还原成网络字节序的字节。
fn addr(hex: &str, fam: Family) -> Option<IpAddr> {
    let words = match fam {
        Family::V4 if hex.len() == 8 => 1,
        Family::V6 if hex.len() == 32 => 4,
        _ => return None,
    };
    let mut bytes = Vec::with_capacity(words * 4);
    for i in 0..words {
        let w = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
        bytes.extend_from_slice(&w.to_ne_bytes());
    }
    Some(match fam {
        Family::V4 => IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
        Family::V6 => IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(bytes).ok()?)),
    })
}

/// `地址:端口`（都是十六进制）。
fn endpoint(s: &str, fam: Family) -> Option<(IpAddr, u16)> {
    let (a, p) = s.split_once(':')?;
    Some((addr(a, fam)?, u16::from_str_radix(p, 16).ok()?))
}

/// 一整份 `/proc/net/tcp` 或 `tcp6` ⇒ 已建立的那几行。表头与认不出的行丢掉。
pub(crate) fn parse_proc_net_tcp(text: &str, fam: Family) -> Vec<TcpLine> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            // sl · local · remote · st · tx:rx · tr:when · retrnsmt · uid · timeout · inode
            if f.len() < 10 || !f[0].ends_with(':') || f[3] != ESTABLISHED {
                return None;
            }
            let (local, local_port) = endpoint(f[1], fam)?;
            let (remote, remote_port) = endpoint(f[2], fam)?;
            let inode = f[9].parse::<u64>().ok().filter(|&i| i != 0)?;
            Some(TcpLine {
                local,
                local_port,
                remote,
                remote_port,
                inode,
            })
        })
        .collect()
}

/// `/proc/<pid>/stat` 一行 ⇒ 父进程号 · 名字 · 启动时刻。名字夹在第一个左括号与**最后一个**右括号之间（名字里可以有括号与空格）。
pub(crate) fn parse_stat(line: &str) -> Option<StatLine> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = line[open + 1..close].to_string();
    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
    // 右括号之后：state(3) ppid(4) … starttime(22)
    let ppid = rest.get(1)?.parse::<u32>().ok()?;
    let start = super::proc::parse_starttime_from_stat(line)?;
    Some(StatLine { ppid, name, start })
}

/// fd 链接 `socket:[N]` ⇒ N。
pub(crate) fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// `/proc` 下的进程号们。
fn pids() -> Result<Vec<u32>, String> {
    let rd = std::fs::read_dir("/proc").map_err(|e| format!("read /proc: {e}"))?;
    Ok(rd
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .collect())
}

/// 想要的那些 inode ⇒ 拥有者进程号（扫得到 fd 的进程里第一个持有它的）。
fn owners(wanted: &[u64], pids: &[u32]) -> HashMap<u64, u32> {
    let mut out = HashMap::new();
    if wanted.is_empty() {
        return out;
    }
    for &pid in pids {
        let Ok(rd) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(link) = std::fs::read_link(e.path()) else {
                continue;
            };
            if let Some(i) = link.to_str().and_then(socket_inode) {
                if wanted.contains(&i) {
                    out.entry(i).or_insert(pid);
                }
            }
        }
        if out.len() == wanted.len() {
            break;
        }
    }
    out
}

/// 现读一次连接表与进程表 ⇒ 那一行 JSON；连接表读不到 ⇒ `Err(原话)`。
pub(crate) fn connection_and_process_tables() -> Result<String, String> {
    let mut lines = Vec::new();
    for (path, fam) in [
        ("/proc/net/tcp", Family::V4),
        ("/proc/net/tcp6", Family::V6),
    ] {
        match std::fs::read_to_string(path) {
            Ok(t) => lines.extend(parse_proc_net_tcp(&t, fam)),
            // 内核没开 IPv6 时没有 tcp6；IPv4 那张必须有。
            Err(e) if fam == Family::V4 => return Err(format!("read {path}: {e}")),
            Err(_) => {}
        }
    }
    let pids = pids()?;
    let inodes: Vec<u64> = lines.iter().map(|l| l.inode).collect();
    let owner = owners(&inodes, &pids);
    let tcp: Vec<serde_json::Value> = lines
        .iter()
        .filter_map(|l| {
            let pid = owner.get(&l.inode)?;
            Some(serde_json::json!({
                "la": l.local.to_string(),
                "lp": l.local_port,
                "ra": l.remote.to_string(),
                "rp": l.remote_port,
                "pid": pid,
            }))
        })
        .collect();
    let proc: Vec<serde_json::Value> = pids
        .iter()
        .filter_map(|&pid| {
            let s = parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
            Some(
                serde_json::json!({ "pid": pid, "ppid": s.ppid, "name": s.name, "start": s.start }),
            )
        })
        .collect();
    Ok(serde_json::json!({ "tcp": tcp, "proc": proc }).to_string())
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/linux_tables_tests.rs"]
mod tests;
