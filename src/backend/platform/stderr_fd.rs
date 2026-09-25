//! 〔NT2 · S1〕**本进程 fd 2 的两件事**：换成指向某份文件 · 问它现在多长 —— 平台那一半（`dup2` / `fstat` 住这里，`platform/` 是唯一许平台原语的层）。
//!
//! 谁要它：`crate::stderr_log`（脱离常驻那条载体的后端把自己的 stderr 落进一份有上限、滚动的文件，`设计/15 §4.7 S1`）。
//! 换 fd 而不是换 `tracing` 的写者：本进程写 stderr 的不止 `tracing` —— `eprintln!`（`[relay]` 那几句）· panic 信息 ·
//! 继承了 stderr 的子进程，fd 这一层一次全接住；而且写是**同步**落盘的，进程 `exit` 那一刻前面的话一个字都不丢
//! （先前那一版是「管子 ＋ 读线程」，`exit(4)` 之前那句「监听口配置不成立」来不及被读线程搬走 —— 真二进制现打逮到，见 `NT2.md §5`）。
//!
//! 非 unix 那一臂是 `Err`：脱离那条载体今天只在 Linux 上有（monitor 那侧 `spawn_detached` 的非 Linux 臂就回 `Err`），
//! Windows 本机后端走被监护的 stdio 载体、stderr 进 monitor 日志 —— 这一格答不上来，照实说（`fallback_guard` 那条纪律）。

/// 让 fd 2 指向 `f` 那份文件（原来的 fd 2 被原子地替掉）。`f` 之后可以丢：fd 2 持着同一个打开的文件。
#[cfg(unix)]
pub(crate) fn point_stderr_at(f: &std::fs::File) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    // SAFETY: `f` 在本函数里活着，它的 fd 有效；`dup2` 只把它复制到 2，不动 `f` 本身。
    if unsafe { libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(format!(
            "换不了 stderr：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// fd 2 此刻指着的那份东西有多长（普通文件才有意义；问不到 ⇒ `None`）。
#[cfg(unix)]
pub(crate) fn stderr_len() -> Option<u64> {
    // SAFETY: `st` 是一块给 `fstat` 填的栈上内存；fd 2 不管指着什么，`fstat` 只读它的元数据。
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(libc::STDERR_FILENO, &mut st) } != 0 {
        return None;
    }
    u64::try_from(st.st_size).ok()
}

/// 非 unix：没有这一格（见模块头注）。
#[cfg(not(unix))]
pub(crate) fn point_stderr_at(_f: &std::fs::File) -> Result<(), String> {
    Err("这台机器不是 unix：不知道怎么把 stderr 换到一份文件上，没换".to_string())
}

/// 非 unix：没有这一格（见模块头注）。
#[cfg(not(unix))]
pub(crate) fn stderr_len() -> Option<u64> {
    None
}
