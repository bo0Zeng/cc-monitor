//! 没有承诺的平台（既不是 Linux 也不是 Windows：macOS 等）的看守形态 —— 一个诚实的空壳，不是一个假实现。
//!
//! `watch_pid_until_exit` 的契约是「进程终止时调 `on_dead`」，这个平台上没有实现。三个选项：
//!
//! | 做法 | 后果 |
//! |---|---|
//! | 立刻调 `on_dead` | 误归档：进程活得好好的，会话被判死。这是最坏的 |
//! | 起个线程轮询 `pid_alive` | 违反零定时器，且 `pid_alive` 在这个平台上同样未实现 |
//! | 什么都不做 + 大声记录 | 会话留在 live 直到 pidfile 删除或断连来收 —— 保守方向 |
//!
//! 选第三个，与 `pidwatch::linux` 里「`poll` 真错误不报死」同一条纪律；平台未实现比一次 syscall 失败更该保守。
//! `tracing::error!` 而不是 `warn!`：缺了一整条判活路径，这个平台上的 backend 只有 pidfile inotify 一条腿。

/// 见模块头注：**这个平台上还没有实现，什么都不做**。
///
/// 参数全部忽略；`on_dead` **永远不会被调用**（刻意的保守方向）。
pub(crate) fn watch_pid_until_exit<F>(pid: u32, expected_start: Option<u64>, on_dead: F)
where
    F: FnOnce() + Send + 'static,
{
    let _ = (expected_start, on_dead);
    tracing::error!(
        "pidfd 看守在本平台未实现（pid {pid}）—— 进程退出**不会**产生死亡事件。\
         会话只能靠 pidfile 删除或断连来收。有真实现的只有 Linux（pidwatch/linux.rs）与 Windows（pidwatch/win32.rs）。"
    );
}
