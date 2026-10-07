//! shell 方言知识只住这里（后端 OS 适配层）：哪种方言 · 引号 · 定义函数 · 导出 / 清除环境 · 以某命令替换进程 ·
//! rc 与 `$PROFILE` 在哪 · 落盘编码 · 内建别名探测 · 这台说哪几种。别名 · 起会话渲染 · 中转环境前缀 · `ccm` 计划都只调它。
//!
//! - [`dialect`]：别名那一族的方言
//! - [`posix`] / [`powershell`]：起会话 · 中转 · `ccm` 计划 · 观测探针要的那几句写法
//! - 本文件：把一串命令交给这台的 POSIX shell 那一跳
//!
//! 判据 `tests/backend/platform/shell_home_guard.rs`：方言专属语法字面量在本目录之外零命中（两向）。
//! 本层不往上依赖：通用层的东西（`ccm` 那个词 · `--` 分界 · 我们那块别名块的正文）作参数交进来。
//!
//! 非 unix 那一臂是 `None`，不是 `cmd /C`：经这条口送出去的是 POSIX shell 脚本（`command -v` 门控、`exec`、`2>/dev/null`），
//! 交给 `cmd.exe` / PowerShell 是另一种语言 ⇒ 诚实地说「本平台没有这一格」，由调用方决定怎么降级。

use crate::platform::child::Child;
pub(crate) mod dialect;
pub(crate) mod posix;
pub(crate) mod powershell;

/// 备一条 `sh -c <脚本>`。**非 unix 上回 `None`** —— 那里没有 `sh`。
///
/// 只负责「备好这条命令」，不起 —— 送出去那一下（`run` ＋ 期限）归调用方，
/// 它还要往上挂环境变量（`UTF8_CLIENT_ENV` 那一族）与决定怎么读回来。
///
/// ⚠ 本函数**不判脚本内容**：脚本是不是 POSIX 语法、跑不跑得动，那是调用方的事。
pub(crate) fn posix_shell(script: &str) -> Option<Child> {
    #[cfg(unix)]
    {
        Some(Child::new("sh").arg("-c").arg(script))
    }
    #[cfg(not(unix))]
    {
        let _ = script;
        None
    }
}

/// 这台的哪一代 PowerShell：两代各读各的 profile 目录、各有一份执行策略（线上名 `powershell` / `pwsh`）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum PsHost {
    /// Windows 自带的 5.1。
    #[serde(rename = "powershell")]
    Desktop,
    /// PowerShell 7。
    #[serde(rename = "pwsh")]
    Core,
}

/// 备一条 `<那一代>.exe -NoProfile -NonInteractive -Command <脚本>`（不弹窗）。
/// **非 Windows 上回 `None`** —— 那里没有自带的 PowerShell（同 [`posix_shell`] 的反面，理由同）。
///
/// 只给**固定脚本**用（`Get-Alias` · 执行策略那两句）；`-NoProfile` 让结果不被用户 profile 左右，`-NonInteractive` 让它绝不等人回车。
/// 剥掉继承来的 `PSExecutionPolicyPreference`：那是父进程给的进程级策略，新开的 PowerShell 窗口没有它。
/// 不 `spawn`，送出去那一下归调用方。
pub(crate) fn powershell_on(host: PsHost, script: &str) -> Option<Child> {
    speaks_powershell().then(|| powershell_command(host, script))
}

fn powershell_command(host: PsHost, script: &str) -> Child {
    // 程序名写成字面量：起进程登记表（`tests/backend/readonly_guard.rs` 那张 `ALLOWED`）按它认是谁。
    let c = match host {
        PsHost::Desktop => Child::new("powershell.exe"),
        PsHost::Core => Child::new("pwsh.exe"),
    };
    c.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env_remove("PSExecutionPolicyPreference")
        .no_console_window()
}

/// 这台后端**说不说 PowerShell** —— 别名方言那一道闸（`assets/aliases::dialect_here`）只问它。
/// 「本机」与「远端」对后端没有区别（本机＝不走 ssh 的远端）：PowerShell ⇔ 这台是 Windows。
pub(crate) fn speaks_powershell() -> bool {
    cfg!(windows)
}

/// 这台机器上「开一个终端窗口跑一串命令」那一串是哪种语言：Windows 是 PowerShell，别处是 POSIX shell
/// （本机起会话的渲染按它挑写法，`control/launch_render/local.rs`）。
pub(crate) const LOCAL_TERMINAL_IS_POWERSHELL: bool = cfg!(windows);

#[cfg(test)]
#[path = "../../../../tests/backend/platform/shell_tests.rs"]
mod tests;
// 方言专属语法字面量只住本目录（两向）。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/shell_home_guard.rs"]
mod home_guard;
