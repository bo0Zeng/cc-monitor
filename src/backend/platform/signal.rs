//! U3（2026-08-01）：**发信号**这一族平台原语。
//!
//! 从 `control/tmux_hook.rs` 下沉 —— 它带着一个裸 `#[cfg(unix)] + libc::kill`，
//! 而 §1.1-1 说 `platform/` 是唯一允许平台原语与平台 cfg 的层。
//! U2 的 Phase D 审计把它列进了「生产段还在 platform 之外的 4 处」，并写明
//! 「§1.1 已裁定 tmux_hook 归 control ⇒ **U3 连它一起处理**，否则 control 里带一个裸 libc 原语」。
//!
//! **身份校验刻意留在调用方**（`tmux_hook`）：那是域判断（「这个 pid 是不是我那个 daemon」，
//! 靠 starttime 比对），不是平台能力。本层只负责「把信号发出去」这一件事。

/// 给 `pid` 发 `SIGUSR1`。返回是否发成功。
///
/// **非 Unix 上恒返回 `false`** —— 与 `pid_alive` 那个「恒 true」的地雷不同，
/// 这里的 `false` 是**保守方向**：发不出去就当没发，调用方（`tmux_hook`）本来就把
/// 「发失败」当作可容忍的竞态（校验之后、发信号之前 daemon 退出了）。
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
