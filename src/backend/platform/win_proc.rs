//! Windows 上「一个 pid 还在不在 · 它哪一刻起的 · 停它」的 Win32 读法 —— 唯一住址（开进程句柄与等它退出住 `pidwatch-core`，这里从那里开）。
//!
//! `proc.rs` 的判活三件在 Windows 上的读法住这里；判定规则一条都不在这里
//! （「exists / captured / current 三者怎么组合成存活」只住 `liveness.rs`）。
//! 用 `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` ＋ `GetExitCodeProcess == STILL_ACTIVE` ＋ `GetProcessTimes`，与 Linux 那一臂同契约：
//! ① 「拒绝访问」算存在：`pid_alive` 的契约是存在性（Linux 的 `/proc/<pid>` 对别的用户的进程照样存在），不是「我能不能碰它」；
//! ② 起始时刻交 FILETIME 原值（UTC、100ns、自 1601），不换成本地 ticks：消费者要的是「同一个读法读两次能不能对上」，不该带进时区与夏令时。
//!
//! 不用 `windows` / `windows-sys` crate：为几个 `kernel32` 函数加一条依赖不如手写 `extern "system"`。签名逐个对着 Win32 文档写：
//! `HANDLE` = 指针宽度（与 `std::os::windows::io::RawHandle` 同形）、`BOOL` = `i32`、`DWORD` = `u32`。
//!
//! 买不到：这份文件只在 Windows 编译时存在，门禁买到的是 `winchk-backend` 的「编得过」；`OpenProcess` 的错误码、`WaitForSingleObject`
//! 在进程退出那一刻醒，都没有真机读数。能在 Linux 上验的只有纯函数那一半（`proc.rs::unix_secs_from_filetime`）。

#![cfg(windows)]

use pidwatch_core::{open, Opened, PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZE};
use std::os::windows::io::{AsRawHandle, OwnedHandle, RawHandle};

/// Win32 `FILETIME`：两段 32 位拼成的 100ns 计数（自 1601-01-01 UTC）。
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetExitCodeProcess(process: RawHandle, exit_code: *mut u32) -> i32;
    fn GetProcessTimes(
        process: RawHandle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn WaitForSingleObject(handle: RawHandle, milliseconds: u32) -> u32;
    fn TerminateProcess(process: RawHandle, exit_code: u32) -> i32;
    fn GetCurrentThread() -> RawHandle;
    fn SetThreadPriority(thread: RawHandle, priority: i32) -> i32;
    /// 本 crate 里关手写拿到的 Win32 句柄（Job · 令牌 …）只走这一处声明。
    pub(crate) fn CloseHandle(handle: RawHandle) -> i32;
}

/// `GetExitCodeProcess` 对还在跑的进程回的那个值。
const STILL_ACTIVE: u32 = 259;
/// `WaitForSingleObject` 的「那个对象被触发了」（进程句柄 = 进程已退出）。
const WAIT_OBJECT_0: u32 = 0;
/// `WaitForSingleObject` 的「期限到了，对象没被触发」。
const WAIT_TIMEOUT: u32 = 0x0000_0102;
/// 允许 `TerminateProcess`。
const PROCESS_TERMINATE: u32 = 0x0000_0001;

/// 只读查询用的句柄（`pid_alive` / `proc_starttime`）。
pub(crate) fn open_for_query(pid: u32) -> Opened {
    open(pid, PROCESS_QUERY_LIMITED_INFORMATION)
}

/// 这个进程**此刻**还在跑吗。`None` = 这一刻问不出来（交给调用方按「读不到不判死」处置）。
///
/// ⚠ 已知的歧义照记：一个**以 259 为退出码**退出的进程会被读成「还在跑」。
/// monitor 侧那一份有同一个歧义（同一个调用），claude 不以 259 退出；不另开一次 `Wait` 去消它 ——
/// 那会多一个「等 0 毫秒」的调用形，读起来像节拍。
pub(crate) fn still_running(h: &OwnedHandle) -> Option<bool> {
    let mut code: u32 = 0;
    // SAFETY：句柄由 `OwnedHandle` 持有、在本次调用期间有效；`code` 是栈上的一个 `u32`。
    let ok = unsafe { GetExitCodeProcess(h.as_raw_handle(), &mut code) };
    (ok != 0).then_some(code == STILL_ACTIVE)
}

/// 这个进程的**创建时刻**（FILETIME 原值：UTC、100ns、自 1601-01-01）。`None` = 读不到。
pub(crate) fn creation_filetime(h: &OwnedHandle) -> Option<u64> {
    let mut creation = FileTime::default();
    let mut exit = FileTime::default();
    let mut kernel = FileTime::default();
    let mut user = FileTime::default();
    // SAFETY：句柄有效（同上）；四个出参都是栈上的 `#[repr(C)]` FILETIME。
    let ok = unsafe {
        GetProcessTimes(
            h.as_raw_handle(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    (ok != 0).then(|| (u64::from(creation.high) << 32) | u64::from(creation.low))
}

/// 开一个「停得了」的句柄：查询 ＋ 等 ＋ 强杀（`platform/signal.rs::stoppable` 的 Windows 臂）。
pub(crate) fn open_for_stop(pid: u32) -> Opened {
    open(
        pid,
        PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE | PROCESS_TERMINATE,
    )
}

/// **至多等 `ms` 毫秒**看它退没退（内核事件，不轮询）。只给一次性子命令 `--resident-stop` 用 ——
/// 登记在 `clippy.toml` 的 `disallowed-methods`，调用处就地放行。`Ok(true)` = 退了；`Ok(false)` = 期限到了还在。
pub(crate) fn wait_within(h: &OwnedHandle, ms: u32) -> Result<bool, std::io::Error> {
    // SAFETY：句柄有效，且带 `SYNCHRONIZE`（由 `open_for_stop` 开出来的）。
    let r = unsafe { WaitForSingleObject(h.as_raw_handle(), ms) };
    match r {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(std::io::Error::last_os_error()),
    }
}

/// 一直等到那个进程退了（进程句柄有信号；只等内核那一个事件，不是定时器）。不取退出码、不关句柄 —— 句柄归调用方
/// （伪终端起的那一个：`platform/child.rs` 先这样不收尸地等，再在锁里收尸）。
pub(crate) fn wait_exited(h: std::os::windows::io::BorrowedHandle<'_>) {
    /// `INFINITE`。
    const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
    // SAFETY：句柄由调用方借给这一次调用，期间有效。
    unsafe { WaitForSingleObject(h.as_raw_handle(), WAIT_FOREVER) };
}

/// 强杀（`TerminateProcess`，退出码 1）。
pub(crate) fn terminate(h: &OwnedHandle) -> Result<(), std::io::Error> {
    // SAFETY：句柄有效，且带 `PROCESS_TERMINATE`（由 `open_for_stop` 开出来的）。
    if unsafe { TerminateProcess(h.as_raw_handle(), 1) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// `pid_alive` 的 Windows 读法（规则写在 `proc.rs::pid_alive` 的头注里，这里只照着开句柄）。
pub(crate) fn exists(pid: u32) -> bool {
    match open_for_query(pid) {
        // 开得到 ⇒ 看退出码；这一刻问不出来 ⇒ 按「读不到不判死」算在。
        Opened::Handle(h) => still_running(&h).unwrap_or(true),
        // 在，只是没权限 —— 与 Linux 的 `/proc/<pid>` 同契约（存在性，不是权限）。
        Opened::Denied => true,
        Opened::Gone(_) => false,
    }
}

/// `proc_starttime` 的 Windows 读法：创建时刻的 FILETIME 原值。开不出句柄 ⇒ `None`（「不知道」）。
pub(crate) fn start_filetime(pid: u32) -> Option<u64> {
    match open_for_query(pid) {
        Opened::Handle(h) => creation_filetime(&h),
        Opened::Denied | Opened::Gone(_) => None,
    }
}

/// 把调用它的这条线程降到 `THREAD_PRIORITY_BELOW_NORMAL`（`proc.rs::lower_this_thread` 的 Windows 臂）。回真 ＝ 降成了。
pub(crate) fn lower_this_thread() -> bool {
    /// `THREAD_PRIORITY_BELOW_NORMAL`。
    const BELOW_NORMAL: i32 = -1;
    // SAFETY: `GetCurrentThread` 回伪句柄（当前线程，不用关）＋ 一个常数。
    unsafe { SetThreadPriority(GetCurrentThread(), BELOW_NORMAL) != 0 }
}
