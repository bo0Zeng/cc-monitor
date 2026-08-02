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
//! - [`proc`]：`/proc` 与进程身份（`pid_alive` / `proc_starttime` / `proc_claude_config_dir` /
//!   `session_alive` / 两个 `/proc` 格式解析器 …）
//! - [`liveness`]：判活的**纯判定表**（`is_same_live_process`）—— U4a 从 `proc` 上提，
//!   因为它是 Windows 侧要复用的那一半（读事实的方式不同，判定规则相同）
//! - [`paths`]：`path_key`（NTFS 大小写折叠 —— **路径**语义，不是 `/proc`）
//! - [`pidwatch`]：`pidfd_open` + [`pidwatch::watch_pid_until_exit`]
//! - [`signal`]：`send_sigusr1`（U3 从 `control/tmux_hook.rs` 下沉）
//!
//! **前三个是从 `watcher.rs` 逐字搬来的**（U2 纯重构，行为逐字不变）。
//! [`signal`] 不是 —— 它是**重写**：原实现内联在 `tmux_hook` 里、失败时 `return 0`；
//! 现在返回 `bool` 由调用方丢弃。语义等价（两条路径旧版都返回 `0`，那个 `return` 是纯提前返回），
//! 但「逐字搬来」这句话覆盖不到它，故单列。

mod fallback_guard;
pub(crate) mod liveness;
pub(crate) mod paths;
pub(crate) mod pidwatch;
pub(crate) mod proc;
pub(crate) mod signal;
