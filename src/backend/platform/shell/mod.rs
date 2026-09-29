//! 〔OSA · `设计/99 §1` V156：「生成alias这个东西是不是也应该后端搞? 这样就可以融入os适配层」〕
//! **shell 方言知识只住这里**（后端 OS 适配层）：哪种方言 · 引号 · 定义函数 · 导出 / 清除环境 · 以某命令替换进程 ·
//! rc 与 `$PROFILE` 在哪 · 落盘编码 · 内建别名探测 · 这台说哪几种。别名 · 起会话渲染 · 中转环境前缀 · `ccm` 计划都只调它。
//!
//! - [`dialect`]：别名那一族的方言（原 `assets/aliases/dialect.rs` 整份）
//! - [`posix`] / [`powershell`]：起会话 · 中转 · `ccm` 计划 · 观测探针要的那几句写法
//! - 本文件：把一串命令交给这台的 shell 那一跳（下面是它自己的来历）
//!
//! 判据 `tests/backend/platform/shell_home_guard.rs`：方言专属语法字面量在本目录之外零命中（两向）。
//! 本层不往上依赖：通用层的东西（`ccm` 那个词 · `--` 分界 · 我们那块别名块的正文）作参数交进来。
//!
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

pub(crate) mod dialect;
pub(crate) mod posix;
pub(crate) mod powershell;

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

/// 〔WF1 · L〕这台的哪一代 PowerShell：两代各读各的 profile 目录、各有一份执行策略（线上名 `powershell` / `pwsh`）。
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

/// 〔MIG-3a · `设计/99 §2.1 ⑬`〕备一条 `<那一代>.exe -NoProfile -NonInteractive -Command <脚本>`（不弹窗）。
/// **非 Windows 上回 `None`** —— 那里没有自带的 PowerShell（同 [`posix_shell`] 的反面，理由同）。
///
/// 只给**固定脚本**用（`Get-Alias` · 执行策略那两句）；`-NoProfile` 让结果不被用户 profile 左右，`-NonInteractive` 让它绝不等人回车。
/// 〔WF1 · L〕剥掉继承来的 `PSExecutionPolicyPreference`：那是父进程给的进程级策略，新开的 PowerShell 窗口没有它。
/// 不 `spawn`，送出去那一下归调用方。
pub(crate) fn powershell_on(host: PsHost, script: &str) -> Option<std::process::Command> {
    speaks_powershell().then(|| powershell_command(host, script))
}

fn powershell_command(host: PsHost, script: &str) -> std::process::Command {
    // 程序名写成字面量：起进程登记表（`tests/backend/readonly_guard.rs` 那张 `ALLOWED`）按它认是谁。
    let mut c = match host {
        PsHost::Desktop => std::process::Command::new("powershell.exe"),
        PsHost::Core => std::process::Command::new("pwsh.exe"),
    };
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    c.env_remove("PSExecutionPolicyPreference");
    hide_console(&mut c);
    c
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
#[path = "../../../../tests/backend/platform/shell_tests.rs"]
mod tests;
// 〔OSA · V156〕方言专属语法字面量只住本目录（两向）。
#[cfg(test)]
#[path = "../../../../tests/backend/platform/shell_home_guard.rs"]
mod home_guard;
