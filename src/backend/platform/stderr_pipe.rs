//! 〔NT2 · S1〕**把本进程的 fd 2 换成一根管子的写端**，交回读端 —— 平台那一半（`dup2` 住这里，`platform/` 是唯一许平台原语的层）。
//!
//! 谁要它：`crate::stderr_log`（脱离常驻那条载体的后端把自己的 stderr 落进一份有上限、滚动的文件，`设计/15 §4.7 S1`）。
//! 换 fd 而不是换 `tracing` 的写者：本进程写 stderr 的不止 `tracing` —— `eprintln!`（`[relay]` 那几句）· panic 信息 ·
//! 继承了 stderr 的子进程，fd 这一层一次全接住。
//!
//! 非 unix 那一臂是 `Err`：脱离那条载体今天只在 Linux 上有（monitor 那侧 `spawn_detached` 的非 Linux 臂就回 `Err`），
//! Windows 本机后端走被监护的 stdio 载体、stderr 进 monitor 日志 —— 这一格答不上来，照实说（`fallback_guard` 那条纪律）。

/// 建一根管子、把写端装到 fd 2（原来的 fd 2 被替掉）、交回读端。
///
/// 读端带 `CLOEXEC`（`std::io::pipe` 的缺省）⇒ 本进程起的子进程拿不到它；fd 2 是 `dup2` 出来的、**不带** `CLOEXEC`
/// ⇒ 继承 stderr 的子进程写的也进这根管子（那正是要的）。
#[cfg(unix)]
pub(crate) fn capture_stderr() -> Result<std::io::PipeReader, String> {
    use std::os::fd::AsRawFd;
    let (rd, wr) = std::io::pipe().map_err(|e| format!("建不了管子：{e}"))?;
    // SAFETY: `wr` 在本函数里活着，它的 fd 有效；`dup2` 把它复制到 2（原子地替掉原来那个），不动 `wr` 本身。
    if unsafe { libc::dup2(wr.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(format!(
            "换不了 stderr：{}",
            std::io::Error::last_os_error()
        ));
    }
    // fd 2 已经持着写端的一份复制；这一份不再要。
    drop(wr);
    Ok(rd)
}

/// 非 unix：没有这一格（见模块头注）。
#[cfg(not(unix))]
pub(crate) fn capture_stderr() -> Result<std::io::PipeReader, String> {
    Err("这台机器不是 unix：不知道怎么把 stderr 接进管子，没接".to_string())
}
