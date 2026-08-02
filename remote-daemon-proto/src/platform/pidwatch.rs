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

/// P2：`pidfd_open(2)`。绑的是**进程实例本身**而不是 pid 数字 ⇒ **PID 复用在机制上
/// 不存在**（不是"检测得更准"，是"无从发生"）。这是本工作区唯一一条正确性改进。
///
/// 需 Linux 5.3+（本机 7.0）。失败最常见的是 `ESRCH`——目标在 open 之前就没了。
pub(crate) fn pidfd_open(pid: u32) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY：`SYS_pidfd_open` 只读地为目标进程创建一个 fd，不解引用任何指针。
    // 返回值 <0 时按 errno 处理、不构造 OwnedFd；>=0 时它是一个我们独占的新 fd，
    // 交给 OwnedFd 接管所有权（唯一持有者，Drop 时 close）。
    let rc = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0 as libc::c_uint) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY：rc 是刚由内核分配、无人持有的合法 fd。
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(rc as i32) })
}

/// 给一个进程挂 pidfd 看守，进程终止时调 `on_dead`。**零轮询**——线程阻塞在无超时
/// `poll(2)` 上，由**内核**在目标进程终止时唤醒。
///
/// `expected_start` 是加表时抓到的 procStart 基线，用于挡「读 pidfile → `pidfd_open`」
/// 之间发生的 PID 复用。
///
/// `on_dead` 在上面头注列的三条路径上各调用一次；`poll` 真错误那条**不调**。
pub(crate) fn watch_pid_until_exit<F>(pid: u32, expected_start: Option<u64>, on_dead: F)
where
    F: FnOnce() + Send + 'static,
{
    let fd = match pidfd_open(pid) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::debug!("pidfd_open pid {pid} 失败（目标已不在？）: {e}");
            on_dead();
            return;
        }
    };
    // 判据 2：open 之后复核身份（挡 pidfile 读取 → pidfd_open 之间的 PID 复用）。
    // 用既有的 `session_alive`（存在性 + 同实例），语义正是这里要的；顺带覆盖
    // "pidfd 开成功但进程在这一瞬已退出"。
    if !super::proc::session_alive(pid, expected_start) {
        tracing::warn!("pidfd 开到的 pid {pid} 与基线不符（PID 复用）⇒ 当死");
        on_dead();
        return;
    }
    let builder = std::thread::Builder::new().name(format!("pidfd-{pid}"));
    let spawned = builder.spawn(move || {
        use std::os::fd::AsRawFd;
        let raw = fd.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd: raw,
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            // SAFETY：pfd 是栈上的一个合法 pollfd，nfds=1 与之匹配；timeout=-1 = 无限等。
            // `fd` 的所有权在本闭包里，poll 期间不会被 close。
            let n = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, -1) };
            if n >= 0 {
                on_dead();
                return;
            }
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // EINTR：接着等
            }
            // 真错误：**不**报死（见头注）。
            tracing::warn!("pidfd poll pid {pid} 失败、放弃看守（不报死）: {err}");
            return;
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("起 pidfd 看守线程失败 pid {pid}: {e}");
    }
}

#[cfg(test)]
mod tests {
    /// **「`poll` 真错误不判死」这条语义的结构性钉子。**
    ///
    /// # 为什么是源码扫描而不是普通测试
    ///
    /// 那条路径要求 `poll(2)` 返回负值**且不是 `EINTR`` —— 在测试里可靠地制造它需要
    /// 伪造一个坏 fd 或注入 syscall 失败，成本远超收益。而它恰恰是**切分时最容易丢的一条**：
    /// 另外三条都「发死亡事件」，只有它不发；重写的人很自然会把四条统一成「都发」，
    /// 而后果是**一次系统调用失败就把活着的会话误归档**（与 `is_same_live_process` 头注
    /// 那条「瞬时读失败绝不误归档」是同一条纪律）。
    ///
    /// ⇒ 退而求其次：钉住**代码形状** —— EINTR 之后那段里不许出现 `on_dead`。
    /// 这挡不住逻辑改写，但挡得住「顺手统一成都发」这个真实的失败模式。
    /// Phase D 审计确认过：这条路径今天**零测试覆盖**，是 U2 之前就有的缺口。
    #[test]
    fn poll_hard_error_must_not_report_dead() {
        let src = include_str!("pidwatch.rs");
        let prod = crate::guard_support::production_code(src);
        let i = prod
            .find("ErrorKind::Interrupted")
            .expect("找不到 EINTR 分支 —— 本钉子的锚点没了，先确认 poll 循环还在");
        // 从 EINTR 判断到函数收尾这一段 = 「真错误」的处理段。
        let tail = &prod[i..];
        let end = tail.find("\n    });").unwrap_or(tail.len());
        let hard_error_arm = &tail[..end];
        assert!(
            !hard_error_arm.contains("on_dead"),
            "`poll` 真错误的处理段里出现了 `on_dead` —— 那条路径**必须不判死**。\n             宁可让会话留在 live、等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档。\n             实际那一段：\n{hard_error_arm}"
        );
        assert!(
            hard_error_arm.contains("放弃看守"),
            "「放弃看守（不报死）」那句 warn 不见了 —— 它是这条路径唯一的可观测痕迹"
        );
    }
}
