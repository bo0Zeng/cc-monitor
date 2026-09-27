//! U3（2026-08-01）：**发信号**这一族平台原语。
//!
//! 从 `control/tmux_hook.rs` 下沉 —— 它带着一个裸 `#[cfg(unix)] + libc::kill`，
//! 而 §1.1-1 说 `platform/` 是唯一允许平台原语与平台 cfg 的层。
//! U2 的 Phase D 审计把它列进了「生产段还在 platform 之外的 4 处」，并写明
//! 「§1.1 已裁定 tmux_hook 归 control ⇒ **U3 连它一起处理**，否则 control 里带一个裸 libc 原语」。
//!
//! **身份校验刻意留在调用方**（`tmux_hook`）：那是域判断（「这个 pid 是不是我那个后端」，
//! 靠 starttime 比对），不是平台能力。本层只负责「把信号发出去」这一件事。

/// 〔HX1〕**等一次停机信号**（unix：SIGTERM 或 SIGINT；别处：Ctrl-C）。从 `main.rs` 下沉（那里原有一个同形的等信号函数，已删）——
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

/// 〔HOST〕给 `pid` 发 `SIGTERM`（常驻后端收到后排空再退）。**调用方必须已核过身份**（同 [`send_sigusr1`]）。非 Unix 恒 `false`。
pub(crate) fn send_sigterm(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: 见 `send_sigusr1` 头注 —— 身份校验是调用方的责任，这里只做系统调用。
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// 〔RM1f〕给**整个进程组** `pgid` 发 `SIGKILL`。返回是否发成功。
///
/// 谁要它：`plugin::invoke::run_abortable` —— 调用方放弃等待时，要连同 `timeout(1)` 前缀
/// 起的那个孙进程一起收掉（只杀直接子进程会留下干活的那一个）。组是起的时候自己立的
/// （`detach::detach` 那一格），组号 == 子进程 pid。
///
/// **非 Unix 上恒返回 `false`**（保守方向，同 [`send_sigusr1`]）：那边没有「一组」这一格，
/// 调用方另有 `kill_on_drop` 杀直接子进程，本函数发不出去就当没发。
///
/// # SAFETY
///
/// `killpg` 是 async-signal-safe 的 libc 调用。**调用方必须保证那个组还是它立的那一个** ——
/// 组长 pid 在被收尸之前不会被复用（`run_abortable` 只在收尸之前开这一枪）。
pub(crate) fn kill_group(pgid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: 见头注 —— 组号的来历由调用方保证，这里只做系统调用。
        unsafe { libc::killpg(pgid as libc::pid_t, libc::SIGKILL) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pgid;
        false
    }
}
