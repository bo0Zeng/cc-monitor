//! 发信号这一族平台原语（`platform/` 是唯一允许平台原语与平台 cfg 的层）。
//! 身份校验留在调用方（`control/tmux_hook.rs`）：「这个 pid 是不是我那个后端」是域判断，不是平台能力。本层只负责把信号发出去。

/// **等一次停机信号**（unix：SIGTERM 或 SIGINT；别处：Ctrl-C）。从 `main.rs` 下沉（那里原有一个同形的等信号函数，已删）——
/// 那一段带平台 cfg，而流模式的收场（`inbound::exit_after_drain`）也要它（排空时再来一次 ⇒ 不等了）。
///
/// ★ **监听在调用的这一刻就登记好**，返回的 future 只负责等 —— 写成 `async fn` 的话，登记要等第一次被 poll，
/// 建好之后、poll 之前到的那一次信号就漏了。
/// 装不上某一个处理器 ⇒ 说出来，退到只等另一个（原 `main.rs` 那两支的行为逐字保留）。
pub(crate) fn shutdown_listener() -> impl std::future::Future<Output = ()> + Send + 'static {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let term = signal(SignalKind::terminate());
        let int = signal(SignalKind::interrupt());
        async move {
            match (term, int) {
                (Ok(mut t), Ok(mut i)) => {
                    tokio::select! {
                        _ = t.recv() => {}
                        _ = i.recv() => {}
                    }
                }
                (Ok(mut t), Err(e)) => {
                    tracing::error!("failed to install SIGINT handler: {e}");
                    let _ = t.recv().await;
                }
                (Err(e), _) => {
                    tracing::error!("failed to install SIGTERM handler: {e}");
                    // Fall back to Ctrl-C only so we still shut down on SIGINT.
                    let _ = tokio::signal::ctrl_c().await;
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        async {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

/// 给 `pid` 发 `SIGUSR1`。返回是否发成功。
///
/// **非 Unix 上恒返回 `false`** —— 与 `pid_alive` 那个「恒 true」的地雷不同，
/// 这里的 `false` 是**保守方向**：发不出去就当没发，调用方（`tmux_hook`）本来就把
/// 「发失败」当作可容忍的竞态（校验之后、发信号之前后端退出了）。
///
/// # SAFETY
///
/// `kill` 是 async-signal-safe 的 libc 调用。**调用方必须已经校验过 pid 的身份**
/// —— pid 会被复用，给一个无关进程发 SIGUSR1 轻则无效、重则终止它
/// （很多程序把 SIGUSR1 当自定义控制信号，默认处置就是终止）。
pub(crate) fn send_sigusr1(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: 见头注 —— 身份校验是调用方的责任，这里只做系统调用。
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGUSR1) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// **一个要被停的进程的把手** —— 「请它收尾 → 宽限期内等 → 强杀」三步都经它（`control/resident.rs::stop_pid`）。
/// Linux 是 pidfd：信号经它发、退出经它等，拿到把手之后 pid 被复用也打不到别人；Windows 是进程句柄（没有「请它收尾」那一格）。
/// 只给一次性子命令用：带期限的等待登记在 `no_timer_guard::REGISTERED_ONE_SHOT_CLI_WAITS`。
pub(crate) struct Stoppable {
    #[cfg(target_os = "linux")]
    fd: std::os::fd::OwnedFd,
    #[cfg(windows)]
    h: std::os::windows::io::OwnedHandle,
}

/// 拿 `pid` 的把手。`Ok(None)` = 它已经不在了。
pub(crate) fn stoppable(pid: u32) -> Result<Option<Stoppable>, String> {
    let failed = |e: String| {
        copy_core::copy_text(
            "beStop.handle.failed",
            &[("pid", &pid.to_string()), ("e", &e)],
        )
    };
    #[cfg(target_os = "linux")]
    {
        match super::pidwatch::pidfd_open(pid) {
            Ok(fd) => Ok(Some(Stoppable { fd })),
            Err(e) if e.raw_os_error() == Some(libc::ESRCH) => Ok(None),
            Err(e) => Err(failed(e.to_string())),
        }
    }
    #[cfg(windows)]
    {
        use super::win_proc::Opened;
        match super::win_proc::open_for_stop(pid) {
            Opened::Handle(h) => Ok(Some(Stoppable { h })),
            Opened::Gone(_) => Ok(None),
            Opened::Denied => Err(failed("access denied".into())),
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = failed;
        Err(copy_core::copy_text("beStop.platform.unsupported", &[]))
    }
}

impl Stoppable {
    /// Linux：经 pidfd 发一个信号。进程已退 ⇒ 当发到了（它本来就不在了）。
    #[cfg(target_os = "linux")]
    fn send(&self, sig: libc::c_int) -> Result<(), String> {
        use std::os::fd::AsRawFd;
        // SAFETY：`pidfd_send_signal(2)`：fd 是本把手独占的 pidfd，info 传空指针 = 与 kill(2) 同义，flags 0。
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                sig,
                std::ptr::null::<libc::siginfo_t>(),
                0 as libc::c_uint,
            )
        };
        if rc == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        Err(copy_core::copy_text(
            "beStop.signal.failed",
            &[("sig", &sig.to_string()), ("e", &e.to_string())],
        ))
    }

    /// 请它收尾（SIGTERM）。Windows 没有这一格 ⇒ `Err`，调用方直接走强杀并如实报「强杀」。
    pub(crate) fn ask_to_finish(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            self.send(libc::SIGTERM)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(copy_core::copy_text("beStop.term.none", &[]))
        }
    }

    /// 强杀（SIGKILL / `TerminateProcess`）。
    pub(crate) fn kill(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            self.send(libc::SIGKILL)
        }
        #[cfg(windows)]
        {
            super::win_proc::terminate(&self.h).map_err(|e| {
                copy_core::copy_text(
                    "beStop.signal.failed",
                    &[("sig", "TerminateProcess"), ("e", &e.to_string())],
                )
            })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Err(copy_core::copy_text("beStop.platform.unsupported", &[]))
        }
    }

    /// **至多等 `ms` 毫秒**看它退没退：阻塞在内核事件上（pidfd 可读 / 句柄被触发），不轮询、不醒来看。
    /// `Ok(true)` = 退了（Linux 上退了还没被收尸的也算）；`Ok(false)` = 期限到了还在。
    pub(crate) fn exited_within(&self, ms: u32) -> Result<bool, String> {
        let failed = |e: String| copy_core::copy_text("beStop.wait.failed", &[("e", &e)]);
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let ms = libc::c_int::try_from(ms).unwrap_or(libc::c_int::MAX);
            let mut pfd = libc::pollfd {
                fd: self.fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // EINTR：一次性子命令没装信号处理器，几乎不会来；来了就再等一整段（至多三次，有界）。
            for _ in 0..3 {
                // SAFETY：pfd 是栈上的一个合法 pollfd，nfds=1 与之匹配；fd 由 self 持有，等的期间不会被 close。
                let n = unsafe { libc::poll(&mut pfd, 1, ms) };
                if n >= 0 {
                    return Ok(n > 0);
                }
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::Interrupted {
                    return Err(failed(e.to_string()));
                }
            }
            Err(failed("EINTR".into()))
        }
        #[cfg(windows)]
        {
            super::win_proc::wait_within(&self.h, ms).map_err(|e| failed(e.to_string()))
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (ms, failed);
            Err(copy_core::copy_text("beStop.platform.unsupported", &[]))
        }
    }
}
