//! Windows 实现：开一个带 `SYNCHRONIZE` 的进程句柄 ＋ 无超时地等它被触发。开进程句柄那一段（三种结局）也住这里，
//! 后端 `platform/win_proc.rs` 的查询句柄 · 停得了的句柄从这里开。
//!
//! 整个文件 `#![cfg(windows)]`，由 `lib.rs` 那一行 `#[cfg(windows)] mod win32;` 选进来。
//! 看守的形状与 `linux.rs` **逐形对拍**，一条都不多、一条都不少：
//!
//! | | `linux.rs` | 本文件 |
//! |---|---|---|
//! | 判死 ① 开不出来 | `pidfd_open` 失败（`ESRCH`）⇒ `on_dead` | 开句柄回「不在」⇒ `on_dead` |
//! | 判死 ② 开到的是冒名者 | 开完再问 `still_same` ⇒ 不是 `on_dead` | 同一个 `still_same`，同一处位置 |
//! | 判死 ③ 醒了 | `poll(pidfd, POLLIN, -1)` 返回 ⇒ `on_dead` | `WaitForSingleObject(句柄, 一直等)` 触发 ⇒ `on_dead` |
//! | **不**判死 | `poll` 真错误（非 `EINTR`）⇒ 记一行、放弃看守 | 等句柄出错 ⇒ 记一行、放弃看守 |
//! | Windows 独有的一格 | —（`pidfd_open` 不做权限检查） | 开句柄被**拒绝访问** ⇒ 进程**在**，只是我们等不了它 ⇒ **不判死**，放弃看守 |
//!
//! 最后那一格是 Windows 才有的：Linux 的 `pidfd_open` 对任何看得见的 pid 都开得出来，
//! Windows 的 `OpenProcess` 会因权限失败。它与「`poll` 真错误」同一个处置、同一个理由 ——
//! 宁可让调用方那条留着、等别的事件来收，也不把一个活着的进程判死（`lib.rs` 头注）。
//!
//! ★ **PID 复用在机制上不存在**（与 `pidfd` 同一个性质）：握着进程句柄的这段时间里，
//! 系统不会把这个 pid 发给别的进程 ⇒ 判死 ② 只需要做一次（开句柄之后），不需要周期复查。
//!
//! 不用 `windows` / `windows-sys` crate：为两个 `kernel32` 函数加一条依赖不如手写 `extern "system"`。
//! `HANDLE` = 指针宽度（与 `std::os::windows::io::RawHandle` 同形）、`BOOL` = `i32`、`DWORD` = `u32`。
//!
//! 🚫 **买不到**：本机是 Linux，这份文件在门禁上只买到 Windows 目标的「编得过」；`OpenProcess` 的错误码、
//! `WaitForSingleObject` 在进程退出那一刻醒，都没有真机读数。与 `linux.rs` 的逐形对拍住
//! `tests/common/pidwatch-core/windows_shape_tests.rs`（把源码当数据读）。

#![cfg(windows)]

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> RawHandle;
    fn WaitForSingleObject(handle: RawHandle, milliseconds: u32) -> u32;
}

/// 只读查询（退出码 · 时间）。跨完整性级别也开得出来。
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
/// 允许 `WaitForSingleObject` 等这个句柄。
pub const SYNCHRONIZE: u32 = 0x0010_0000;
/// `ERROR_ACCESS_DENIED`：进程**在**，只是我们没权限开它。
const ERROR_ACCESS_DENIED: i32 = 5;
/// `WaitForSingleObject` 的「一直等」—— 不是节拍，是「只等内核那一个事件」（同 Linux 臂 `poll` 的 `-1`）。
const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
/// `WaitForSingleObject` 的「那个对象被触发了」（进程句柄 = 进程已退出）。
const WAIT_OBJECT_0: u32 = 0;

/// 开一个进程句柄的三种结局 —— **三种，不压成 `Option`**：
/// 「没权限」与「不在了」在判活里方向相反（前者是存在，后者是死），压成一个 `None` 就是「一个值装了两件事」。
pub enum Opened {
    /// 开到了。句柄归调用方，`Drop` 时关（`OwnedHandle`）。
    Handle(OwnedHandle),
    /// 进程在，但本进程没权限开它。
    Denied,
    /// 开不出来、也不是权限问题 ⇒ 当它不在（pid 不存在时 Win32 回 `ERROR_INVALID_PARAMETER`）。
    Gone(std::io::Error),
}

/// 按 `access` 开 `pid` 的进程句柄。
pub fn open(pid: u32, access: u32) -> Opened {
    if pid == 0 {
        // pid 0 是「System Idle Process」，不是任何会话的进程；Win32 对它回参数错。
        return Opened::Gone(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    // SAFETY：`OpenProcess` 只按值收三个整数，不解引用任何指针；失败回空指针、错误码在线程的
    // last-error 上（紧接着读，中间不调别的 Win32）。成功时回的是一个**我们独占**的新句柄，
    // 交给 `OwnedHandle` 接管（唯一持有者，`Drop` 时 `CloseHandle`）。
    let raw = unsafe { OpenProcess(access, 0, pid) };
    if raw.is_null() {
        let e = std::io::Error::last_os_error();
        return if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
            Opened::Denied
        } else {
            Opened::Gone(e)
        };
    }
    // SAFETY：见上 —— 非空即是刚分配、无人持有的合法进程句柄。
    Opened::Handle(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// **阻塞到这个进程退出**。`Ok` = 它退出了；`Err` = 等这件事本身出错了（调用方**不许**当它死了）。
///
/// 零节拍：一直等内核那一个事件，不带任何超时（同 Linux 臂 `poll(pidfd, POLLIN, -1)`）。
fn wait_for_exit(h: &OwnedHandle) -> Result<(), std::io::Error> {
    // SAFETY：句柄有效，且带 `SYNCHRONIZE`（看守那一处开的）。
    let r = unsafe { WaitForSingleObject(h.as_raw_handle(), WAIT_FOREVER) };
    if r == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// 给一个进程挂看守，进程终止时调 `on_dead`。**零轮询** —— 线程阻塞在一个不带超时的等待上，
/// 由**内核**在目标进程终止时唤醒（同 `linux.rs` 那条）。
///
/// `still_same` 在句柄开到之后调一次：答「不是那个进程了」⇒ 当死（挡「记下进程号 → 开句柄」之间的 PID 复用）。
pub fn watch_pid_until_exit<S, F>(pid: u32, still_same: S, on_dead: F)
where
    S: FnOnce() -> bool,
    F: FnOnce() + Send + 'static,
{
    // ★ 握着它的这段时间里，**这个 pid 不会被系统复用**（进程对象被我们的句柄钉着）。
    let handle = match open(pid, PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE) {
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
    // 判死 ②：开完之后复核身份（挡记下进程号 → 开句柄之间的 PID 复用），与 `linux.rs` 同一处。
    if !still_same() {
        tracing::warn!("开到的 pid {pid} 与基线不符（PID 复用）或已退出 ⇒ 当死");
        on_dead();
        return;
    }
    let builder = std::thread::Builder::new().name(format!("pidwait-{pid}"));
    let spawned = builder.spawn(move || match wait_for_exit(&handle) {
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
