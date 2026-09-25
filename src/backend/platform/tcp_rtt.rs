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
//! - 其余（macOS / Windows）：**答不上来就说不知道**（`None`）—— 判准那一侧对「不知道」有明文处置（按「远」压）。
//!   macOS 的 `TCP_CONNECTION_INFO` 与 Windows 的 `SIO_TCP_INFO` 都能答这一问，**没接**（本机量不到，不写没量过的代码）。

/// 这条 TCP 此刻的平滑往返时间（微秒）。读不到 / 平台答不上来 ⇒ `None`。
pub(crate) fn rtt_us(stream: &tokio::net::TcpStream) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        linux_rtt_us(stream)
    }
    #[cfg(not(target_os = "linux"))]
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
