//! 等一个进程退出：[`watch_pid_until_exit`]。后端（会话的 claude 退了 ⇒ 归档）与 monitor（登记着窗口的 shell 退了 ⇒ 删那条登记）同一份；
//! 「醒了做什么」是调用方的事，本包不知道。
//!
//! # 三条判死路径 + 一条不判死的路径
//!
//! 1. 开不出来（Linux `pidfd_open` 失败，`ESRCH` 等）⇒ 目标已不在 ⇒ 判死。
//! 2. 开到之后调调用方给的 `still_same` 复核「还是不是那个进程」：不是 = 在「记下进程号」与「开」之间发生了 PID 复用，
//!    开到的是冒名者 ⇒ 判死。复核只在开的时候做一次，之后靠内核（握着 pidfd / 进程句柄的这段时间里不存在复用）。
//!    怎么判「同一个」（进程号 ＋ 起始时刻，各侧各有读法）归调用方，本包只定它在哪一刻问。
//! 3. 起线程不带期限地等（Linux `poll(pidfd, POLLIN, -1)`）；醒了判死。
//!
//! 等这件事本身真出错（Linux `poll` 非 `EINTR` 的错）时刻意不判死：宁可让调用方那条留着、等别的事件来收，
//! 也不因一次系统调用失败就把活着的进程判死。这条没有普通测试能覆盖（要让 `poll(2)` 真出错），由 `linux.rs` 的源码扫描钉住。
//!
//! 线程数的界：调用方每看守一个进程起一条，活到那个进程退出为止。
//!
//! # 按平台分文件
//!
//! `linux` 是 pidfd 实现，`win32` 是 Windows 实现（开带 `SYNCHRONIZE` 的进程句柄 ＋ 不带超时地等；开句柄那一段也住它，后端的
//! Windows 读法从这里取），`fallback` 是没有承诺的平台（macOS 等）的诚实空壳（见它自己的头注）。
//! 分文件而不是在函数里塞 `#[cfg]`：这一族的平台差异是整套机制不同，不是某一行不同。
//! `win32` 与 `linux` 逐形对拍（三条判死 ＋ 一条不判死 ＋ Windows 独有的「拒绝访问 ⇒ 不判死」），判据住
//! `tests/common/pidwatch-core/windows_shape_tests.rs`；Windows 那一份只买到编得过 ＋ 源码对拍，真机零读数。

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win32;

#[cfg(not(any(target_os = "linux", windows)))]
pub use fallback::watch_pid_until_exit;
#[cfg(target_os = "linux")]
pub use linux::{pidfd_open, watch_pid_until_exit};
#[cfg(windows)]
pub use win32::{open, watch_pid_until_exit, Opened, PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZE};

/// 非 Linux 那份空壳的两条承诺，只能靠源码级守卫钉：`fallback.rs` 在本机根本不编译，行为判据结构上不可能存在。
#[cfg(test)]
#[path = "../../../../tests/common/pidwatch-core/fallback_shape_tests.rs"]
mod fallback_shape_tests;

/// **Windows 那一份与 Linux 那一份逐形对拍**（把源码当数据读 —— 本机编不到它）。
#[cfg(test)]
#[path = "../../../../tests/common/pidwatch-core/windows_shape_tests.rs"]
mod windows_shape_tests;

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
