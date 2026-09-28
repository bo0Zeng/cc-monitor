//! `K-R55`（2026-09-11）：**「把一条命令串交给 POSIX shell」这一族平台原语**。
//!
//! # 它从哪来
//!
//! `K-R52` 立 [`super::cfgless_guard`] 那一拍，A2（编得过、跑不对）堆里挂着
//! `observe/watcher.rs` 的两处 `Command::new("sh")`，签字栏逐字写着
//! 「该进适配层（`K33` 裁定二），今天没进 …… `K-R52` 的写区不含 `observe/`」。
//! 本模块就是那两处的新住址。
//!
//! # 为什么非 unix 那一臂是 `None`，不是 `cmd /C`
//!
//! 经这条口送出去的是**POSIX shell 脚本**（`command -v` 门控、`exec`、`2>/dev/null`），
//! 交给 `cmd.exe` / PowerShell 不是「另一种写法」，是**另一种语言** ——
//! 编一个 `cmd /C` 出来等于给一个答不上来的问题编一个看起来无害的答案
//! （`fallback_guard` 头注里 `pid_alive` 那个地雷的同一形）。
//! ⇒ 这一臂诚实地说「本平台没有这一格」，由调用方决定怎么降级。
//!
//! # 行为面：**这一拍在 unix 上逐字节不变，在非 unix 上也不变**
//!
//! 搬之前，非 unix 上 `Command::new("sh").output()` 会返回 `Err`（找不到 `sh`），
//! 两个调用方各自落进自己的 `Err` 臂（`Unobservable` / `(None, None)`）。
//! 搬之后那两条路走的是 `None` 臂，**落点逐字相同** ——
//! 变的只有一件事：先前那是「碰巧撞出来的」，现在是**写出来的**。

/// 备一条 `sh -c <脚本>`。**非 unix 上回 `None`** —— 那里没有 `sh`。
///
/// 只负责「备好这条命令」，不 `spawn`、不 `output` —— 送出去那一下归调用方，
/// 它还要往上挂环境变量（`UTF8_CLIENT_ENV` 那一族）与决定怎么读回来。
///
/// ⚠ 本函数**不判脚本内容**：脚本是不是 POSIX 语法、跑不跑得动，那是调用方的事。
pub(crate) fn posix_shell(script: &str) -> Option<std::process::Command> {
    #[cfg(unix)]
    {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(script);
        Some(c)
    }
    #[cfg(not(unix))]
    {
        let _ = script;
        None
    }
}

/// 〔MIG-3a · `设计/99 §2.1 ⑬`〕备一条 `powershell.exe -NoProfile -NonInteractive -Command <脚本>`（不弹窗）。
/// **非 Windows 上回 `None`** —— 那里没有自带的 PowerShell（同 [`posix_shell`] 的反面，理由同）。
///
/// 只给**固定脚本**用（今天唯一的调用方：别名方言问内建别名 `Get-Alias`，`assets/aliases/dialect.rs`）；
/// `-NoProfile` 让结果不被用户 profile 左右，`-NonInteractive` 让它绝不等人回车。不 `spawn`，送出去那一下归调用方。
pub(crate) fn powershell_readonly(script: &str) -> Option<std::process::Command> {
    if !speaks_powershell() {
        return None;
    }
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    hide_console(&mut c);
    Some(c)
}

/// `CREATE_NO_WINDOW`：挡掉 Windows 给控制台程序新开的那个黑框。别处没有控制台窗口这一说 ⇒ 什么都不做。
#[cfg(windows)]
fn hide_console(c: &mut std::process::Command) {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_c: &mut std::process::Command) {}

/// 〔MIG-3a · 主会话 09-27 裁〕这台后端**说不说 PowerShell** —— 别名方言那一道闸（`assets/aliases::dialect_here`）只问它。
/// 「本机」与「远端」对后端没有区别（本机＝不走 ssh 的远端）：PowerShell ⇔ 这台是 Windows。
pub(crate) fn speaks_powershell() -> bool {
    cfg!(windows)
}

/// 〔MIG-2〕这台机器上「开一个终端窗口跑一串命令」那一串是哪种语言：Windows 是 PowerShell，别处是 POSIX shell
/// （本机起会话的渲染按它挑写法，`control/launch_render/local.rs`）。
pub(crate) const LOCAL_TERMINAL_IS_POWERSHELL: bool = cfg!(windows);

#[cfg(test)]
#[path = "../../../tests/backend/platform/shell_tests.rs"]
mod tests;
