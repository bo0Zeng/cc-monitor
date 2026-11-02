//! 只听回环的那一个地址。

use std::net::{IpAddr, Ipv4Addr};

/// 只听回环。**字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见。
/// 中转的宿主（`relay/listen.rs`）绑口用这一个。
pub const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
