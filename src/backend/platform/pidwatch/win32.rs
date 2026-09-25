//! 〔WN1 · U4b 后半〕Windows 实现：开一个带 `SYNCHRONIZE` 的进程句柄 ＋ 无超时地等它被触发。
//!
//! 整个文件 `#![cfg(windows)]`，由 `mod.rs` 那一行 `#[cfg(windows)] mod win32;` 选进来。
//! Win32 读法本身（开句柄 · 等句柄）住 `platform/win_proc.rs`，本文件只写**看守的形状** ——
//! 与 `linux.rs` **逐形对拍**，一条都不多、一条都不少：
//!
//! | | `linux.rs` | 本文件 |
//! |---|---|---|
//! | 判死 ① 开不出来 | `pidfd_open` 失败（`ESRCH`）⇒ `on_dead` | 开句柄回「不在」⇒ `on_dead` |
//! | 判死 ② 开到的是冒名者 | 开完再 `session_alive` 复核基线 ⇒ 不符 `on_dead` | 同一个 `session_alive`，同一处位置 |
//! | 判死 ③ 醒了 | `poll(pidfd, POLLIN, -1)` 返回 ⇒ `on_dead` | `WaitForSingleObject(句柄, 一直等)` 触发 ⇒ `on_dead` |
//! | **不**判死 | `poll` 真错误（非 `EINTR`）⇒ 记一行、放弃看守 | 等句柄出错 ⇒ 记一行、放弃看守 |
//! | Windows 独有的一格 | —（`pidfd_open` 不做权限检查） | 开句柄被**拒绝访问** ⇒ 进程**在**，只是我们等不了它 ⇒ **不判死**，放弃看守 |
//!
//! 最后那一格是 Windows 才有的：Linux 的 `pidfd_open` 对任何看得见的 pid 都开得出来，
//! Windows 的 `OpenProcess` 会因权限失败。它与「`poll` 真错误」同一个处置、同一个理由 ——
//! 宁可让会话留在 live、等 pidfile 删除或断连来收，也不把一个活着的进程判死（`mod.rs` 头注）。
//!
//! ★ **PID 复用在机制上不存在**（与 `pidfd` 同一个性质）：握着进程句柄的这段时间里，
//! 系统不会把这个 pid 发给别的进程 ⇒ 判死 ② 只需要做一次（开句柄之后），不需要周期复查。
//!
//! 线程数的界与 Linux 同：每个被追踪的 (pidfile, pid) 至多一条，活到目标进程退出为止。
//!
//! 🚫 **买不到**：本机是 Linux，这份文件在门禁上只买到 `winchk-backend` 的「编得过」；
//! 「真 Windows 上关掉终端窗口 ⇒ claude 被强杀 ⇒ 这条线程醒 ⇒ 会话归档」**一格都没有读数**
//! （本路不碰 Win11 虚拟机，`99 §2 ⑤` 未拍）。与 `linux.rs` 的逐形对拍住
//! `tests/backend/platform/pidwatch_windows_shape_tests.rs`（把源码当数据读）。

#![cfg(windows)]

use crate::platform::win_proc::{self, Opened};

/// 这一份看守**会**在进程退出时调 `on_dead` —— `mod.rs::death_events_available` 的 Windows 那一格
/// 读的就是它。它是一句**声明**，背书它的是 `tests/backend/platform/pidwatch_windows_shape_tests.rs`
/// （判死路径与 `linux.rs` 逐形相等）；真机那一维没有读数。
pub(crate) const WAKES_ON_EXIT: bool = true;

/// 给一个进程挂看守，进程终止时调 `on_dead`。**零轮询** —— 线程阻塞在一个不带超时的等待上，
/// 由**内核**在目标进程终止时唤醒（同 `linux.rs` 那条）。
///
/// `expected_start` 是加表时抓到的起始时刻基线（FILETIME 原值，`proc.rs::proc_starttime` 的 Windows 臂），
/// 用于挡「读 pidfile → 开句柄」之间发生的 PID 复用。
pub(crate) fn watch_pid_until_exit<F>(pid: u32, expected_start: Option<u64>, on_dead: F)
where
    F: FnOnce() + Send + 'static,
{
    let handle = match win_proc::open_for_wait(pid) {
        Opened::Handle(h) => h,
        // 判死 ①：开不出来、也不是权限问题 ⇒ 目标已不在。
        Opened::Gone(e) => {
            tracing::debug!("开进程句柄失败 pid {pid}（目标已不在？）: {e}");
            on_dead();
            return;
        }
        // Windows 独有：进程在，我们等不了它 ⇒ **不报死**（见头注表最后一行）。
        Opened::Denied => {
            tracing::warn!("没有权限等 pid {pid}（拒绝访问）、放弃看守（不报死）");
            return;
        }
    };
    // 判死 ②：开完之后复核身份（挡 pidfile 读取 → 开句柄之间的 PID 复用）。
    // 用既有的 `session_alive`（存在性 + 同实例），与 `linux.rs` 同一处、同一个函数。
    if !crate::platform::proc::session_alive(pid, expected_start) {
        tracing::warn!("开到的 pid {pid} 与基线不符（PID 复用）或已退出 ⇒ 当死");
        on_dead();
        return;
    }
    let builder = std::thread::Builder::new().name(format!("pidwait-{pid}"));
    let spawned = builder.spawn(move || match win_proc::wait_for_exit(&handle) {
        // 判死 ③：进程句柄被触发 = 进程已退出。
        Ok(()) => on_dead(),
        // 真错误：**不**报死（见头注）。
        Err(err) => {
            tracing::warn!("等 pid {pid} 退出这件事本身出错、放弃看守（不报死）: {err}");
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("起进程看守线程失败 pid {pid}: {e}");
    }
}
