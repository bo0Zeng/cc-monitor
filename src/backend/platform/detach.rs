//! **把一个要起的子进程从这一趟会话里摘出去**的平台原语。
//!
//! # 谁要它
//!
//! `control/resident.rs`：`--resident-ensure` 起那台的常驻后端 —— 起它的是一趟 SSH exec，SSH 一断那一趟就没了，
//! 而常驻后端（与它进程内的中转）**不许跟着一起没了**。⇒ 放进**自己的进程组**，不与起它的那一趟共享作业控制与信号。
//! 先前的第一个调用方（起脱离的 `--relay` 那一处）随那一族删了。
//!
//! 另一个调用方：`plugin::invoke::run_abortable` —— 同一格原语、另一个理由：让插件子进程**自成一组**，
//! 调用方放弃等待时 `signal::kill_group` 才有一组可杀（`timeout` 前缀下干活的是孙进程）。
//! 那边对 `Err` 的读法是「没有一组可杀，只靠 `kill_on_drop` 收直接子进程」，不当失败。
//!
//! # 为什么非 unix 那一臂是 `Err`，不是「什么都不做」
//!
//! Windows 上对应物是创建标志（另一套语义：控制台、作业对象），本仓在 Windows 上**一次都没验过**。
//! ⇒ 这一臂诚实地说「本平台没有这一格」（`fallback_guard` 头注那条纪律：答不上来的问题不给一个看起来无害的答案），
//! 由调用方把它说成一次失败（`--resident-ensure` 答 `unsupported`，monitor 明说「远端只支持 Unix」）。

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
    Err(copy_core::copy_text("beDetach.detach.notUnix", &[]))
}
