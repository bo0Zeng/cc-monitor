//! U2（2026-08-01）：**pidfd 看守**这一族平台原语。
//!
//! # 这里为什么要切一刀
//!
//! 搬家前 `spawn_pid_watcher` 一个函数里同时装着两件事：
//! ① `pidfd_open` + 身份复核 + `poll(2)` + 起线程（**平台**）；
//! ② 醒了往哪个 channel 发哪一种 `WatchEvent`（**observe 的域知识**）。
//!
//! 主计划把回边判成了 `session_alive`（「`spawn_pid_watcher:228` 调它」），
//! **那条判断偏了一个函数** —— `session_alive` = `pid_alive` + `proc_starttime` +
//! `is_same_live_process`，三者分别是平台原语、平台原语、纯函数，整条都在 platform 域内。
//! 真正的回边是 `spawn_pid_watcher` 自己依赖 `PidWatchTarget` / `WatchEvent` 这两个
//! observe 域类型。**一个平台原语不该知道「醒了要往哪个 channel 发什么帧」。**
//!
//! ⇒ 切开而不是参数化谓词：本模块只提供 [`watch_pid_until_exit`]，
//! `watcher.rs` 留一层薄包装把 `on_dead` 实现成 `tx.send(target.death_event(pid))`。
//!
//! # 三条判死路径 + 一条**不**判死的路径
//!
//! 判死（调 `on_dead`）：① `pidfd_open` 失败 ② 身份复核不符（PID 复用）③ `poll` 醒。
//! **不判死**：`poll` 返回真错误（非 EINTR）—— 原实现逐字写着「真错误：**不**报死」，
//! 这条语义在切分时必须原样保住，切错了就是「看守线程挂了却把会话判成活的/死的」。
//! 这条**没有普通测试能覆盖**（要让 `poll(2)` 真出错），故由本文件末尾的源码扫描钉住。
//!
//! # 三条判据的原文（搬自 `watcher.rs`，一字未改）
//!
//! 三条判据，按顺序：
//! 1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 立刻发 `PidDied`。
//! 2. open 成功后**再读一次** `proc_starttime` 与 add 时捕获的基线比对
//!    （复用既有纯函数 `is_same_live_process`）：不符 = 在"读 pidfile"与
//!    "开 pidfd"之间发生了 PID 复用 ⇒ 我们开到的是冒名者 ⇒ 发 `PidDied`。
//!    **这就是原先那套 procStart 启发式的全部去处**——从"每 2s 复查一遍"
//!    降级为"开 pidfd 时校验一次"，之后靠内核，不再需要周期比对。
//! 3. 起线程 `poll(pidfd, POLLIN, -1)`；醒了发 `PidDied`。
//!
//! **线程数的界**：每个被追踪的 (pidfile, pid) 最多一条，实际是个位数
//! （一台机器上同时活着的 CC 交互会话数）。线程活到目标进程真正退出为止——
//! 若 pidfile 先被删而进程仍在，那条线程会继续等，等到进程退出时发一条
//! **陈旧唤醒**，被消费侧的 pid 比对挡掉（无副作用）。
//!
//! **`poll` 真出错（非 `EINTR`）时刻意不发 `PidDied`**：宁可让会话留在 live、
//! 等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档——与本文件
//! `is_same_live_process` 头注那条「瞬时读失败绝不误归档」同一条纪律。
//!
//! > 这段说明 U2 之前**贴在 `enum PidWatchTarget` 头上**（隔着 enum + impl 才到它描述的
//! > `spawn_pid_watcher`）—— 是 U2 之前就有的错位，U2 把它从「贴错 item」升级成了「跨文件悬空」。
//! > Phase D 审计逮出，搬到它真正描述的代码旁边。

//! ---
//!
//! # U4a：按平台分文件
//!
//! `linux` 是原实现（逐字搬，`#![cfg(target_os = "linux")]`）；
//! `fallback` 是**诚实的空壳**，不是假实现 —— 见它自己的头注。
//!
//! （这两个名字**刻意不用 intra-doc 链接**：两个 mod 各自带 cfg，在任一 target 上只有一个存在，
//! 写成 `[\`fallback\`]` 会在 Linux 上产生一条悬空链接 —— Phase D 审计 重要-5 逮到的正是它。）
//!
//! 分文件而不是在函数里塞 `#[cfg]`：这一族的平台差异是**整套机制不同**
//! （pidfd+poll vs OpenProcess+WaitForSingleObject），不是某一行不同。
//! 塞在一个函数里会让两套实现的 `unsafe` 与所有权推理互相纠缠。

#[cfg(not(target_os = "linux"))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::watch_pid_until_exit;
// `pidfd_open` 只被 `watcher.rs` 的测试段用（生产段的唯一调用点在 `linux.rs` 内部）。
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::watch_pid_until_exit;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use linux::pidfd_open;
