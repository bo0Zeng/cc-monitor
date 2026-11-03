//! **这台 Windows 的已建立 TCP 连接表 ＋ 进程表**（↗ 那一问的系统事实，`dial/terminal_processes.rs` 判）—— 直调系统接口：
//! `GetExtendedTcpTable`（IPv4 / IPv6，带拥有者进程号）· `CreateToolhelp32Snapshot`（进程号 · 父进程号 · 名字）·
//! `GetProcessTimes`（启动时刻，`win_proc::start_filetime`）。直调系统接口（起一趟 PowerShell 就要 1–2 s），
//! 这里几十毫秒。只读：不读任何进程的命令行与内存。
//!
//! 产出一行 JSON（`{tcp:[{la,lp,ra,rp,pid}…], proc:[{pid,ppid,name,start}…]}`，
//! `start` 是 FILETIME；开不出句柄的进程 ⇒ 0），解析与判定照旧只在 `terminal_processes.rs`。
//! 签名照 `win_proc.rs` 先例手写 `extern "system"`（`DWORD` = `u32`、`HANDLE` = 指针宽）。

#![cfg(windows)]

use std::net::{Ipv4Addr, Ipv6Addr};
const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;
/// `TCP_TABLE_OWNER_PID_CONNECTIONS`：除了在听的那些，全部（带拥有者进程号）。
const TCP_TABLE_OWNER_PID_CONNECTIONS: u32 = 4;
/// `MIB_TCP_STATE_ESTAB`。
const STATE_ESTABLISHED: u32 = 5;
const NO_ERROR: u32 = 0;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpRow4 {
    state: u32,
    local_addr: u32,
    local_port: u32,
    remote_addr: u32,
    remote_port: u32,
    pid: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpRow6 {
    local_addr: [u8; 16],
    local_scope: u32,
    local_port: u32,
    remote_addr: [u8; 16],
    remote_scope: u32,
    remote_port: u32,
    state: u32,
    pid: u32,
}

#[link(name = "iphlpapi")]
extern "system" {
    fn GetExtendedTcpTable(
        table: *mut core::ffi::c_void,
        size: *mut u32,
        order: i32,
        af: u32,
        class: u32,
        reserved: u32,
    ) -> u32;
}

/// 端口那一格：低 16 位，网络字节序。
fn port(raw: u32) -> u16 {
    u16::from_be((raw & 0xffff) as u16)
}

/// 一族地址的整张表（原始字节）：先问要多大，再取；两次之间表长了就再来一次。
fn tcp_table_bytes(af: u32) -> Result<Vec<u8>, String> {
    let mut size: u32 = 0;
    for _ in 0..4 {
        let mut buf = vec![0u8; size as usize];
        let p = if buf.is_empty() {
            std::ptr::null_mut()
        } else {
            buf.as_mut_ptr().cast()
        };
        // SAFETY：`p` 指向 `size` 字节的可写缓冲（或为空、`size` = 0 时只问大小）；系统只往里写不超过 `size` 字节。
        let rc =
            unsafe { GetExtendedTcpTable(p, &mut size, 0, af, TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
        match rc {
            NO_ERROR if !buf.is_empty() => return Ok(buf),
            NO_ERROR => return Ok(vec![0u8; 4]),
            ERROR_INSUFFICIENT_BUFFER => continue,
            other => return Err(format!("GetExtendedTcpTable({af}) = {other}")),
        }
    }
    Err(format!("GetExtendedTcpTable({af}): table kept growing"))
}

/// 表头一个 `DWORD` 条数，后面紧跟定长的行。
fn rows<T: Copy>(buf: &[u8]) -> Vec<T> {
    let n = buf
        .get(..4)
        .map(|b| u32::from_ne_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .unwrap_or(0);
    // 行从第一个按行对齐的位置起（IPv4 行全是 `u32`，IPv6 行同样按 4 对齐 ⇒ 紧跟在 4 字节表头之后）。
    let start = 4;
    let size = std::mem::size_of::<T>();
    (0..n)
        .filter_map(|i| {
            let at = start + i * size;
            let chunk = buf.get(at..at + size)?;
            // SAFETY：`chunk` 恰好 `size_of::<T>()` 字节、`T` 是全整数的 `repr(C)` 行（任何位型都合法）；按未对齐读取。
            Some(unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast::<T>()) })
        })
        .collect()
}

/// 已建立的连接（两族）。
fn established() -> Result<Vec<serde_json::Value>, String> {
    let mut out = Vec::new();
    for r in rows::<TcpRow4>(&tcp_table_bytes(AF_INET)?) {
        if r.state == STATE_ESTABLISHED {
            out.push(serde_json::json!({
                "la": Ipv4Addr::from(r.local_addr.to_ne_bytes()).to_string(),
                "lp": port(r.local_port),
                "ra": Ipv4Addr::from(r.remote_addr.to_ne_bytes()).to_string(),
                "rp": port(r.remote_port),
                "pid": r.pid,
            }));
        }
    }
    for r in rows::<TcpRow6>(&tcp_table_bytes(AF_INET6)?) {
        if r.state == STATE_ESTABLISHED {
            out.push(serde_json::json!({
                "la": Ipv6Addr::from(r.local_addr).to_string(),
                "lp": port(r.local_port),
                "ra": Ipv6Addr::from(r.remote_addr).to_string(),
                "rp": port(r.remote_port),
                "pid": r.pid,
            }));
        }
    }
    Ok(out)
}

/// 进程表四格（进程号 · 父进程号 · 名字取 `platform_core::proc::processes` 那一趟快照，monitor 同一份）。启动时刻开不出句柄 ⇒ 0（系统没给）。
fn processes() -> Result<Vec<serde_json::Value>, String> {
    let table = platform_core::proc::processes().map_err(|e| format!("process snapshot: {e}"))?;
    Ok(table
        .into_iter()
        .map(|p| {
            let start = super::win_proc::start_filetime(p.pid).unwrap_or(0);
            serde_json::json!({
                "pid": p.pid,
                "ppid": p.ppid,
                "name": p.name,
                "start": start,
            })
        })
        .collect())
}

/// 这个进程的父进程号（现拍一次进程快照；拍不成 · 没有这个进程 ⇒ `None`）。agent 自问往上找会话用（`platform::proc::parent_of`）。
pub(crate) fn parent_of(pid: u32) -> Option<u32> {
    platform_core::proc::processes()
        .ok()?
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.ppid)
}

/// 现问一次连接表与进程表 ⇒ 那一行 JSON；哪一张没问成 ⇒ `Err(原话)`。
pub(crate) fn connection_and_process_tables() -> Result<String, String> {
    let tcp = established()?;
    let proc = processes()?;
    Ok(serde_json::json!({ "tcp": tcp, "proc": proc }).to_string())
}
