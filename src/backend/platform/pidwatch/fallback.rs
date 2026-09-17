//! U4a（2026-08-01）：**非 Linux 的看守形态 —— 一个诚实的空壳，不是一个假实现。**
//!
//! # 它为什么什么都不做，而不是「尽力而为」
//!
//! `watch_pid_until_exit` 的契约是「进程终止时调 `on_dead`」。这个平台上还没有实现
//! （真形态是 `OpenProcess` + `WaitForSingleObject`，属 **U4b**，需要 Windows 真机验证 ——
//! 主计划 U4 行自己写着「等价性仓里无实测，第一步先验」）。
//!
//! 那这里该怎么办？三个选项，只有一个是诚实的：
//!
//! | 做法 | 后果 |
//! |---|---|
//! | 立刻调 `on_dead` | **误归档**：进程活得好好的，会话被判死。这是最坏的 |
//! | 起个线程轮询 `pid_alive` | 违反 §41 零定时器铁律，且 `pid_alive` 在这个平台上同样未实现 |
//! | **什么都不做 + 大声记录** | 会话留在 live 直到 pidfile 删除或断连来收 —— **保守方向** |
//!
//! 选第三个，与本 crate 既有的那条纪律一致：`pidwatch::linux` 里「`poll` 真错误**不**报死」
//! 的理由逐字就是「宁可让会话留在 live、等 pidfile 删除或断连来收，也不因一次系统调用失败
//! 就误归档」。平台未实现比一次 syscall 失败更该保守，不是更不该。
//!
//! # `tracing::error!` 而不是 `warn!`
//!
//! 这不是「可容忍的降级」，是**缺了一整条判活路径**。U4b 落地前，这个平台上的 daemon
//! 只有 pidfile inotify 一条腿。级别要与事实相称。

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
         会话只能靠 pidfile 删除或断连来收。真实现见 U4b（OpenProcess + WaitForSingleObject）。"
    );
}
