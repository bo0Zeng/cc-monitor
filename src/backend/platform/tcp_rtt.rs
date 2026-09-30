//! 〔NT1 · 2026-09-24〕**一条已连上的 TCP 此刻的往返时间 —— 问内核，不自己掐表。**
//!
//! 用户 V23（`设计/99 §1`，逐字）：「今天每台机器只有一条连接可以看情况多开. 智能一点. 这是属于 ssh 优化的部分.
//! 智能多开链接\压缩等等」。压缩「开不开」的判准（`dial/connect.rs::compression_for`）要知道这一跳**真的远不远**：
//! 按地址段判是错的（用户的远端大半走 EasyTier 覆盖网，地址全在 RFC 1918 私网段、RTT 12–577 ms，`NT1.md §0.3`）。
//!
//! 为什么问内核：TCP 三次握手那一来一回，内核已经量过了（Linux `TCP_INFO.tcpi_rtt`，单位微秒）；
//! 本 crate 又禁掐表（`no_timer_guard` 的禁用表里有取当前时刻那一形，而且没有登记口）⇒ 读内核的样本是唯一的路。
//!
//! # 平台
//!
//! - Linux：`getsockopt(IPPROTO_TCP, TCP_INFO)`，读 `tcpi_rtt`（平滑 RTT，握手之后就有第一份样本）。
//! - Windows：〔P2 · `设计/99 §2.3` WF2 报备〕`WSAIoctl(SIO_TCP_INFO)`（Windows 10 1703 起），读 `TCP_INFO_v0.RttUs`；
//!   更老的系统那一问失败 ⇒ `None`。照 `win_proc.rs` 先例手写 `extern "system"`，不为一个函数加 `windows-sys`。
//! - 其余（macOS）：**答不上来就说不知道**（`None`）—— 判准那一侧对「不知道」有明文处置（按「远」压）。
//!
//! 两臂同一条：内核回 0 = 还没有样本，那是「不知道」，不是「0 微秒」。

/// 这条 TCP 此刻的平滑往返时间（微秒）。读不到 / 平台答不上来 ⇒ `None`。
pub(crate) fn rtt_us(stream: &tokio::net::TcpStream) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        linux_rtt_us(stream)
    }
    #[cfg(windows)]
    {
        windows_rtt_us(stream)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = stream;
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_rtt_us(stream: &tokio::net::TcpStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    // SAFETY：`tcp_info` 是纯数据的 C 结构体，全零是合法值；`getsockopt` 只往我们给的那块内存里写、至多 `len` 字节。
    let mut info: libc::tcp_info = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::tcp_info>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_INFO,
            (&mut info as *mut libc::tcp_info).cast::<libc::c_void>(),
            &mut len,
        )
    };
    // `tcpi_rtt == 0` = 内核还没有样本（没握过手的 socket）：那不是「0 微秒」，是「不知道」。
    (rc == 0 && info.tcpi_rtt > 0).then_some(info.tcpi_rtt)
}

// ─── Windows：`SIO_TCP_INFO` ───────────────────────────────────────────────────
// 结构体与控制码照 SDK（`mstcpip.h` 的 `TCP_INFO_v0` · `_WSAIORW(IOC_VENDOR, 39)`）。非 Windows 也编这两样：
// 布局与控制码由 `tcp_rtt_tests` 在每个平台上钉（本机买不到真 Windows，错一个偏移只能靠那一条先红）。

/// `mstcpip.h::TCP_INFO_v0`。只读 `rtt_us`，其余字段只为占住布局。
#[repr(C)]
#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct TcpInfoV0 {
    state: i32,
    mss: u32,
    connection_time_ms: u64,
    timestamps_enabled: u8,
    rtt_us: u32,
    min_rtt_us: u32,
    bytes_in_flight: u32,
    cwnd: u32,
    snd_wnd: u32,
    rcv_wnd: u32,
    rcv_buf: u32,
    bytes_out: u64,
    bytes_in: u64,
    bytes_reordered: u32,
    bytes_retrans: u32,
    fast_retrans: u32,
    dup_acks_in: u32,
    timeout_episodes: u32,
    syn_retrans: u8,
}

/// `SIO_TCP_INFO` = `_WSAIORW(IOC_VENDOR, 39)`。
#[cfg_attr(not(windows), allow(dead_code))]
const SIO_TCP_INFO: u32 = 0xD800_0027;

#[cfg(windows)]
#[link(name = "ws2_32")]
extern "system" {
    // SOCKET = UINT_PTR；重叠与完成例程两格恒传空（同步那一形）。
    fn WSAIoctl(
        s: usize,
        io_control_code: u32,
        in_buffer: *mut core::ffi::c_void,
        in_len: u32,
        out_buffer: *mut core::ffi::c_void,
        out_len: u32,
        bytes_returned: *mut u32,
        overlapped: *mut core::ffi::c_void,
        completion_routine: *mut core::ffi::c_void,
    ) -> i32;
}

#[cfg(windows)]
fn windows_rtt_us(stream: &tokio::net::TcpStream) -> Option<u32> {
    use std::os::windows::io::AsRawSocket;
    let mut version: u32 = 0;
    let mut info = TcpInfoV0::default();
    let mut returned: u32 = 0;
    // SAFETY：入参是一个 `u32` 版本号、出参是一块 `TcpInfoV0` 大小的内存，长度都如实交给内核；不走重叠 I/O。
    let rc = unsafe {
        WSAIoctl(
            stream.as_raw_socket() as usize,
            SIO_TCP_INFO,
            (&mut version as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
            (&mut info as *mut TcpInfoV0).cast(),
            std::mem::size_of::<TcpInfoV0>() as u32,
            &mut returned,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    // `RttUs == 0` 同 Linux 那一臂：没有样本 ⇒ 不知道。
    (rc == 0 && info.rtt_us > 0).then_some(info.rtt_us)
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/tcp_rtt_tests.rs"]
mod tcp_rtt_tests;
