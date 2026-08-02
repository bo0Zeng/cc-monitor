//! Linux 实现：`pidfd_open(2)` + 无超时 `poll(2)`。**从 `platform/pidwatch.rs` 逐字搬来。**
//!
//! 整个文件 `#![cfg(target_os = "linux")]` —— U4a 之前它是无条件编译的，
//! 那正是 daemon 在 Windows 上 12 个错里 11 个的来源（`SYS_pidfd_open` / `std::os::fd` /
//! `libc::poll` / `pollfd` / `POLLIN` / `pid_t`，**一个 cfg 都没有** —— 计划自审 §0.5-3
//! 说的「cfg 位置扫描抓不到它」指的就是这里）。

#![cfg(target_os = "linux")]

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
    if !crate::platform::proc::session_alive(pid, expected_start) {
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
        let src = include_str!("linux.rs");
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
