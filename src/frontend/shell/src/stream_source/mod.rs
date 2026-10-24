//! 会话流从哪来：远端那台后端的流（经本机常驻后端的链路）与本机后端的流，进同一条收口。
//!
//! 各份分工：`config` 远端配置 · `version` 能力与版本 · `snapshot` 旁路快照 ·
//! `exec` 一次性远端 exec 与限长读行 · `frame` 按类型解帧 · `dispatch` 一帧落到哪（本机远端同一处）· `batch` 攒批与收行 ·
//! `local` 本机那条流 · `run` 远端那台的宿主（重连循环本身住 `resident-client`）。外面一律写 `crate::stream_source::X`。
//!
//! 本模块不拨 SSH：链路只经宿主 `dial_host`，读代理应答的是通信层成员 `ssh_link`
//! （所以它不登记为通信层成员，理由住 `comm_boundary_registry_tests::TRANSPORT_LEFT_OUTSIDE`）。

mod batch;
mod config;
mod dispatch;
mod exec;
mod frame;
mod local;
mod run;
mod snapshot;

pub(crate) use batch::*;
pub(crate) use config::*;
pub(crate) use dispatch::*;
pub(crate) use exec::*;
pub(crate) use frame::*;
pub(crate) use local::*;
pub(crate) use run::*;
pub(crate) use snapshot::*;

// 拨号判据（界面进程零拨号）住测试树，按路径挂进来。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/dial_move_judge.rs"]
pub(crate) mod dial_move_judge;

/// 重连循环只有一个诞生点（`resident-client` 的 `feed.rs`），monitor 不许再长出自己的。
#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/one_loop_guard.rs"]
mod one_loop_guard;
