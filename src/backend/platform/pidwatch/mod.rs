//! pidfd 看守这一族平台原语。本模块只提供 [`watch_pid_until_exit`]；「醒了往哪个 channel 发哪一种 `WatchEvent`」是 observe 的域知识，
//! `watcher.rs` 留一层薄包装把 `on_dead` 实现成 `tx.send(target.death_event(pid))`。
//!
//! # 三条判死路径 + 一条不判死的路径
//!
//! 1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 判死。
//! 2. open 成功后再读一次 `proc_starttime` 与 add 时捕获的基线比对（`is_same_live_process`）：不符 = 在「读 pidfile」与「开 pidfd」之间
//!    发生了 PID 复用，开到的是冒名者 ⇒ 判死。procStart 校验只在开 pidfd 时做一次，之后靠内核。
//! 3. 起线程 `poll(pidfd, POLLIN, -1)`；醒了判死。
//!
//! `poll` 真出错（非 `EINTR`）时刻意不判死：宁可让会话留在 live、等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档。
//! 这条没有普通测试能覆盖（要让 `poll(2)` 真出错），由本文件末尾的源码扫描钉住。
//!
//! 线程数的界：每个被追踪的 (pidfile, pid) 最多一条（一台机器上同时活着的交互会话数）。pidfile 先被删而进程仍在时，
//! 那条线程等到进程退出发一条陈旧唤醒，被消费侧的 pid 比对挡掉。

//!
//! ---
//!
//! # 按平台分文件
//!
//! `linux` 是 pidfd 实现，`win32` 是 Windows 实现（开带 `SYNCHRONIZE` 的进程句柄 ＋ 不带超时地等；Win32 读法住 `platform/win_proc.rs`），
//! `fallback` 是没有承诺的平台（macOS 等）的诚实空壳（见它自己的头注）。不写 intra-doc 链接：几个 mod 各自带 cfg，任一 target 上只有一个存在。
//! 分文件而不是在函数里塞 `#[cfg]`：这一族的平台差异是整套机制不同，不是某一行不同。
//! `win32` 与 `linux` 逐形对拍（三条判死 ＋ 一条不判死 ＋ Windows 独有的「拒绝访问 ⇒ 不判死」），判据住
//! `tests/backend/platform/pidwatch_windows_shape_tests.rs`；Windows 那一份只买到编得过 ＋ 源码对拍，真机零读数。

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win32;

#[cfg(target_os = "linux")]
pub(crate) use linux::watch_pid_until_exit;
// `pidfd_open`：`watcher.rs` 的测试段 ＋`platform/signal.rs::stoppable`（一次性子命令 `--resident-stop` 的进程把手）。
#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) use fallback::watch_pid_until_exit;
#[cfg(target_os = "linux")]
pub(crate) use linux::pidfd_open;
#[cfg(windows)]
pub(crate) use win32::watch_pid_until_exit;

/// 非 Linux 那份空壳的两条承诺，只能靠源码级守卫钉：`fallback.rs` 在本机根本不编译，行为判据结构上不可能存在。
///
/// 钉的两条：不立刻调 `on_dead()`（那是最坏的选项：进程活得好好的、会话被判死）· 缺判活路径要 `tracing::error!`、不许降成 `warn!`。
/// 变异这两处后端全套都照样绿，所以必须钉；「起个轮询线程」那条已有 `no_timer_guard` 管，不重复钉。
/// 用 `pin_line`（整行相等）而不是 `contains`：子串匹配会让「在同一行后面追加一个调用」溜过去。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/pidwatch_fallback_shape_tests.rs"]
mod fallback_shape_tests;

/// **Windows 那一份与 Linux 那一份逐形对拍**（把源码当数据读 —— 本机编不到它）。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/pidwatch_windows_shape_tests.rs"]
mod windows_shape_tests;
