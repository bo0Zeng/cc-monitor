//! 〔WN1 · U4b〕**Windows 上「一个 pid 还在不在 · 它哪一刻起的 · 等它死」的 Win32 读法** —— 唯一住址。
//!
//! # 为什么有这份文件
//!
//! `proc.rs` 的判活三件（`pid_alive` / `proc_starttime` / `start_epoch_from_ticks`）与
//! `pidwatch` 的看守，在非 Linux 上从 U4a 起就是空壳（`pid_alive` 甚至是 `unimplemented!()`）。
//! 而 V105 之后 Windows 本机**一定**有一个后端进程在跑 ⇒ 它的 `watcher` 每见一份
//! `sessions/<PID>.json` 就调一次 `pid_alive` ⇒ **见到第一个 claude 会话就 panic**。
//! 这份文件把那几件的 Windows 读法补上；**判定规则一条都不在这里**（翻译官只翻译读法，
//! `01 §3.1`）—— 「exists / captured / current 三者怎么组合成存活」仍只住 `liveness.rs`。
//!
//! # 身份那一半是照搬，不是新写
//!
//! monitor 侧当年 `session_map.rs` 那份进程判活（〔LOC1b · 4D〕随本机判活改由本机后端的帧来删了）的 `cfg(windows)` 那支
//! （`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` ＋ `GetExitCodeProcess == STILL_ACTIVE`
//! ＋ `GetProcessTimes`）在 Windows 用户机上跑过很久（`tests/bridge/rust_timer_registry_tests.rs`
//! 那条「F12 解锁闹钟」的头注逐字）。本文件用的是**同一组 Win32 调用、同一个访问掩码**。
//! 差别只有两处，都是为了与 Linux 那一臂同契约：
//! ① **「拒绝访问」算存在**：Linux 的 `/proc/<pid>` 对别的用户的进程照样存在，
//!    `pid_alive` 的契约是「存在性」，不是「我能不能碰它」；monitor 那一份把它算成死（它问的是另一件事）。
//! ② **起始时刻交 FILETIME 原值**（UTC、100ns、自 1601），不换成 .NET 本地 ticks ——
//!    `proc_starttime` 的消费者要的是「同一个读法读两次能不能对上」（`#34` 基线），
//!    换成本地时间就把时区与夏令时带进了一个本该只比相等的值里。
//!
//! # 这里刻意不用 `windows` / `windows-sys` crate
//!
//! 本 crate 今天一条 Windows 专属依赖都没有；为四个 `kernel32` 函数加一条依赖（还要动 lock）
//! 不如照 `src/bridge/src/utils.rs::to_net_local_ticks` 那处先例手写 `extern "system"`。
//! 签名逐个对着 Win32 文档写：`HANDLE` = 指针宽度（与 `std::os::windows::io::RawHandle` 同形）、
//! `BOOL` = `i32`、`DWORD` = `u32`。
//!
//! # 🚫 买不到（写在最前面，别把绿读成验过）
//!
//! 本机是 Linux：这份文件**只在 Windows 编译时存在**，门禁买到的是 `winchk-backend`
//! 那一格的「编得过」。「在真 Windows 上 `OpenProcess` 真的回那几个错误码、
//! `WaitForSingleObject` 真的在进程退出那一刻醒」—— **一格都没有读数**（本路不碰 Win11 虚拟机，
//! `99 §2 ⑤` 未拍）。能在 Linux 上验的只有纯函数那一半（`proc.rs::unix_secs_from_filetime`）
//! 与「看守三条判死路径 ＋ 一条不判死」和 Linux 臂逐形对拍（`pidwatch` 那边的源码判据）。

#![cfg(windows)]

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

/// Win32 `FILETIME`：两段 32 位拼成的 100ns 计数（自 1601-01-01 UTC）。
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> RawHandle;
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
}

/// 只读查询（退出码 · 时间）。跨完整性级别也开得出来 —— monitor 侧用的正是这一个。
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
/// 允许 `WaitForSingleObject` 等这个句柄。
const SYNCHRONIZE: u32 = 0x0010_0000;
/// `GetExitCodeProcess` 对还在跑的进程回的那个值。
const STILL_ACTIVE: u32 = 259;
/// `ERROR_ACCESS_DENIED`：进程**在**，只是我们没权限开它。
const ERROR_ACCESS_DENIED: i32 = 5;
/// `WaitForSingleObject` 的「一直等」—— 不是节拍，是「只等内核那一个事件」（同 Linux 臂 `poll` 的 `-1`）。
const WAIT_FOREVER: u32 = 0xFFFF_FFFF;
/// `WaitForSingleObject` 的「那个对象被触发了」（进程句柄 = 进程已退出）。
const WAIT_OBJECT_0: u32 = 0;
/// `WaitForSingleObject` 的「期限到了，对象没被触发」。
const WAIT_TIMEOUT: u32 = 0x0000_0102;
/// 〔STOP〕允许 `TerminateProcess`。
const PROCESS_TERMINATE: u32 = 0x0000_0001;

/// 开一个进程句柄的三种结局 —— **三种，不压成 `Option`**：
/// 「没权限」与「不在了」在判活里方向相反（前者是存在，后者是死），压成一个 `None` 就是
/// 「一个值装了两件事」（`lib.rs::TmuxPlatform` 头注那条病）。
pub(crate) enum Opened {
    /// 开到了。句柄归调用方，`Drop` 时关（`OwnedHandle`）。
    Handle(OwnedHandle),
    /// 进程在，但本进程没权限开它。
    Denied,
    /// 开不出来、也不是权限问题 ⇒ 当它不在（pid 不存在时 Win32 回 `ERROR_INVALID_PARAMETER`）。
    Gone(std::io::Error),
}

fn open(pid: u32, access: u32) -> Opened {
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

/// 只读查询用的句柄（`pid_alive` / `proc_starttime`）。
pub(crate) fn open_for_query(pid: u32) -> Opened {
    open(pid, PROCESS_QUERY_LIMITED_INFORMATION)
}

/// 看守用的句柄（多一个 `SYNCHRONIZE`）。
///
/// ★ 握着它的这段时间里，**这个 pid 不会被系统复用**（进程对象被我们的句柄钉着）——
/// 与 Linux 臂 `pidfd` 「绑的是进程实例本身」是同一个性质。
pub(crate) fn open_for_wait(pid: u32) -> Opened {
    open(pid, PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE)
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

/// **阻塞到这个进程退出**。`Ok` = 它退出了；`Err` = 等这件事本身出错了（调用方**不许**当它死了）。
///
/// 零节拍：一直等内核那一个事件，不带任何超时（同 Linux 臂 `poll(pidfd, POLLIN, -1)`）。
pub(crate) fn wait_for_exit(h: &OwnedHandle) -> Result<(), std::io::Error> {
    // SAFETY：句柄有效（同上），且带 `SYNCHRONIZE`（由 `open_for_wait` 开出来的）。
    let r = unsafe { WaitForSingleObject(h.as_raw_handle(), WAIT_FOREVER) };
    if r == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// 〔STOP〕开一个「停得了」的句柄：查询 ＋ 等 ＋ 强杀（`platform/signal.rs::stoppable` 的 Windows 臂）。
pub(crate) fn open_for_stop(pid: u32) -> Opened {
    open(
        pid,
        PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE | PROCESS_TERMINATE,
    )
}

/// 〔STOP〕**至多等 `ms` 毫秒**看它退没退（内核事件，不轮询）。只给一次性子命令 `--resident-stop` 用 ——
/// 登记在 `no_timer_guard::REGISTERED_ONE_SHOT_CLI_WAITS`。`Ok(true)` = 退了；`Ok(false)` = 期限到了还在。
pub(crate) fn wait_within(h: &OwnedHandle, ms: u32) -> Result<bool, std::io::Error> {
    // SAFETY：句柄有效，且带 `SYNCHRONIZE`（由 `open_for_stop` 开出来的）。
    let r = unsafe { WaitForSingleObject(h.as_raw_handle(), ms) };
    match r {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(std::io::Error::last_os_error()),
    }
}

/// 〔STOP〕强杀（`TerminateProcess`，退出码 1）。
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
