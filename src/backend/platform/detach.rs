//! 〔RM1a · 第四波〕**把一个要起的子进程从这一趟会话里摘出去**的平台原语。
//!
//! # 谁要它
//!
//! 远端那台机器上的中转（`--relay`）由那台的后端起（`relay-ensure`）。远端后端走 stdio
//! （SSH exec），SSH 一断它就随管道破裂退出 —— 而那台机器上 tmux 里的会话还活着，
//! 它们要的中转**不许跟着后端一起没了**。⇒ 起中转时要把它放进**自己的进程组**，
//! 不与起它的那一趟共享作业控制与信号。
//!
//! # 为什么非 unix 那一臂是 `Err`，不是「什么都不做」
//!
//! Windows 上对应物是创建标志（另一套语义：控制台、作业对象），本仓在 Windows 上**一次都没验过**；
//! 而今天唯一的调用方只对**远端**那台起中转（monitor 从不对本机后端发这条命令，本机中转由 monitor 监护），
//! 远端目标是 Linux。⇒ 这一臂诚实地说「本平台没有这一格」（`fallback_guard` 头注那条纪律：
//! 答不上来的问题不给一个看起来无害的答案），由调用方把它说成一次失败。

/// 把 `cmd` 设成「起出来之后自成一个进程组」。**只改这一格**，stdio / 环境归调用方。
#[cfg(unix)]
pub(crate) fn detach(cmd: &mut std::process::Command) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    Ok(())
}

/// 非 unix：没有这一格（见模块头注）。
#[cfg(not(unix))]
pub(crate) fn detach(cmd: &mut std::process::Command) -> Result<(), String> {
    let _ = cmd;
    Err("这台机器不是 unix：不知道怎么把子进程起成脱离的一组，没起".to_string())
}
