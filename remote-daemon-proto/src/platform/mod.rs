//! U2（2026-08-01）：**`platform/` —— 唯一允许出现平台原语与平台 `cfg` 的地方**（§1.1 第一条解耦线）。
//!
//! # 判据不是「cfg 出现在哪」
//!
//! 计划自审 §0.5-3 已订正过一次：「`platform/` 之外出现平台 cfg 就红」这条机检**是安慰剂** ——
//! 本 crate 在 Windows 上编不过的 12 个错里，头号的 `pidfd_open` **根本没有 cfg**，
//! 它是无条件编译的 Linux-only 代码。
//!
//! **真判据只有一个：跨 target 编译。**
//! `cargo check --all-targets --target x86_64-pc-windows-msvc` 必须绿，且要进 CI
//! （在 ubuntu 上就能跑，`check` 不链接）。那是 **U4** 的 DoD，不是本模块的。
//!
//! # 本层现在装了什么
//!
//! - [`proc`]：`/proc` 与进程身份（`pid_alive` / `proc_starttime` / `session_alive` / `path_key` …）
//! - [`pidwatch`]：`pidfd_open` + [`pidwatch::watch_pid_until_exit`]
//!
//! 都是从 `watcher.rs` **逐字搬来**的 —— U2 是纯重构，行为逐字不变。

pub(crate) mod paths;
pub(crate) mod pidwatch;
pub(crate) mod proc;
pub(crate) mod signal;
