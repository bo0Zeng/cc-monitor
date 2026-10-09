//! 只听回环的那一个地址。

use std::net::{IpAddr, Ipv4Addr};

/// 只听回环。**字面量常量，不是拼出来的** —— 拼出来的地址源码扫描看不见。
/// 常驻监听口（`main.rs` 的接受循环 · `stream/listen.rs` 的纯判定）与中转的宿主（`relay/listen.rs`）绑口都用这一个，本机探口也用它。
pub const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
