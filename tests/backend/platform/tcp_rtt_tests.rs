//! 住址：`设计/99 §2.3` WF2 报备「Windows 读往返时间（`platform/tcp_rtt.rs` 补 `SIO_TCP_INFO`）…发版后另排」· `设计/15 §3.3`「读得到这条 TCP 真实的握手往返 … ⇒ ≥ `COMPRESS_RTT_FLOOR_US`（5 ms）才压；读不到 ⇒ 压」。
//!
//! Windows 臂在本机只编不跑 ⇒ 它交给内核的那块内存必须与 SDK 逐字节同形：错一个偏移，`RttUs` 读到的是别的字段，
//! 判准会被一个假往返时间带偏（不是「读不到」那种安全的错）。期望值取自 SDK 头文件（`mstcpip.h` · `ws2def.h`），不取自被测定义。

use super::*;

/// `TCP_INFO_v0` 88 字节、`RttUs` 在第 20 字节；`SIO_TCP_INFO` = `_WSAIORW(IOC_VENDOR, 39)`。
#[test]
fn the_windows_tcp_info_request_has_the_sdk_shape() {
    assert_eq!(std::mem::size_of::<TcpInfoV0>(), 88, "TCP_INFO_v0 的大小");
    assert_eq!(std::mem::align_of::<TcpInfoV0>(), 8, "TCP_INFO_v0 的对齐");
    assert_eq!(std::mem::offset_of!(TcpInfoV0, rtt_us), 20, "RttUs 的偏移");
    const IOC_IN: u32 = 0x8000_0000;
    const IOC_OUT: u32 = 0x4000_0000;
    const IOC_VENDOR: u32 = 0x1800_0000;
    assert_eq!(SIO_TCP_INFO, IOC_IN | IOC_OUT | IOC_VENDOR | 39);
}
