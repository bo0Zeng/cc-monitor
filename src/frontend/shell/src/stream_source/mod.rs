//! 会话流从哪来：只读本机后端那一条；各台的常驻流归本机后端（`control/feeds.rs`），那台流上的事由它原样转来（`relayed`），进同一条收口。
//!
//! 各份分工：`config` 远端配置 · `snapshot` 旁路快照 ·
//! `exec` 一次性 exec 结果的形状与限长读行 · `frame` 按类型解帧 · `dispatch` 一帧落到哪（本机与各台同一处）· `batch` 攒批与收行 ·
//! `local` 收的那一头（按那台分收口）。外面一律写 `crate::stream_source::X`。
//!
//! 写流 · 切流 · 数退避在这里写不出来：手里只有通道里的行与 `InboundClient`（没有写方法），退避是 `resident_client::feed` 私有的。
//!
//! 本模块不拨 SSH，也不开任何到远端的字节流：只读本机后端那一条
//! （它不是通信层成员：它是业务家，公开面上是会话 / tmux 那一族，握手带期限值）。

mod batch;
mod config;
mod dispatch;
mod exec;
mod frame;
mod local;
mod snapshot;

pub(crate) use batch::*;
pub(crate) use dispatch::*;
pub(crate) use exec::*;
pub(crate) use frame::*;
pub(crate) use local::*;
pub(crate) use snapshot::*;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/stream_flag_gate_tests.rs"]
mod stream_flag_gate_tests;

/// 收的那一头的接缝：overflow 那一句等。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/seam_tests.rs"]
mod seam_tests;

/// 那台断线 → 重连 → 续上之后，订了这台的那几条订阅各收几格（数格、不看墙钟）。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/reconnect_cells_tests.rs"]
mod reconnect_cells_tests;

// 拨号判据（界面进程零拨号）住测试树，按路径挂进来。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/dial_move_judge.rs"]
pub(crate) mod dial_move_judge;
