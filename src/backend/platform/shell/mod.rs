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

/// 起会话那个 shell 的 `PATH` 夹在这一对标记中间（rc 文件往标准输出打的杂话不算进去）。
pub(crate) const SESSION_PATH_MARK: &str = "@@ccm-session-path@@";

/// 问 `PATH` 那一趟的期限（交互 rc 卡住 ⇒ 到点连同整个进程组一起杀掉，算问不出来）。
pub(crate) const SESSION_PATH_WITHIN: crate::platform::child::Deadline =
    crate::platform::child::Deadline::secs(10);

/// **起会话那个 shell** 的 `PATH`：会话在 tmux 窗格里的用户登录 shell 里起（交互、登录；远端开窗那一形是 `bash -lic`），
/// agent 命令按那里的 `PATH` 找；后端进程自己的 `PATH` 不是那一份（经 ssh 非登录 shell 起时常常没有用户级 `bin`）。
/// 「装没装」（足迹）与「这台能起哪几家」（起新会话框）都只经这一处问。
///
/// 问法：用 `$SHELL` 起一次 `-l -i -c` 打出 `PATH`，**按后端生命周期只问一次**（缓存的是 `PATH`，不是答案：
/// 之后装进同一个目录的程序照样查得到）。交互 rc 里的东西会被触发 ⇒ 防法：stdin 接空、`TERM=dumb`、自成一个进程组、
/// 期限到了整组杀（后端经 ssh / 由界面起，本身没有控制终端 ⇒ 自动接 tmux 那一类接不上就退，卡住的到期限收手）；
/// 只问一次 ⇒ rc 里起 ssh-agent 那一类至多多起一个。
/// 没有 `$SHELL` / 起不来 / 超时 / 没打出来 ⇒ `None`（问不出来，调用方不许当成「没有」）。
/// 非 unix：会话在本机终端里起，环境与本进程同源 ⇒ 本进程的 `PATH`。
pub(crate) fn session_shell_path() -> Option<String> {
    #[cfg(unix)]
    {
        static ONE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        ONE.get_or_init(|| {
            let shell = std::env::var("SHELL")
                .ok()
                .filter(|s| !s.trim().is_empty())?;
            ask_session_path(login_shell_asking_path(&shell), SESSION_PATH_WITHIN)
        })
        .clone()
    }
    #[cfg(not(unix))]
    {
        std::env::var("PATH").ok()
    }
}

/// 在起会话那个 shell 里敲裸名字 `prog`，`PATH` 上会起来的是哪个文件、该怎么起它（[`find_session_command`] 的答案）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionCommand {
    /// `PATH` 问不出 / 空 —— 判不了，调用方不许当成「没有」。
    Unknown,
    /// 查了，没有。
    Absent,
    /// 找到了：起它就是 `program` ＋ `lead` ＋ 用户那几个参数。
    Found {
        /// 找到的那个文件（`.ps1` 那一形里是它，`program` 是 PowerShell）。
        file: std::path::PathBuf,
        program: std::ffi::OsString,
        lead: Vec<std::ffi::OsString>,
    },
}

/// 一种装法的壳怎么起。
#[derive(Clone, Copy)]
enum StartBy {
    /// 直接起那个文件。`.cmd` / `.bat` 也走这一形：起进程原语（std）认得这两种后缀、自己经 `cmd.exe /c` 起并按它的规矩转义参数。
    Itself,
    /// `powershell.exe -NoProfile -File <文件>`（执行策略照这台的，不绕过）。
    PowerShellFile,
}

/// 「装没装」（足迹）· 「这台能起哪几家」（起新会话框）· `ccm` 最后起 agent 那一下 —— **只经这一处**找 agent 命令行，
/// 于是「认得出」就是「起得来」。
///
/// - unix：只认原名（直接起）。
/// - Windows：会话开在 PowerShell 里 ⇒ 照 `where.exe` 的顺序：`PATH` 逐目录、同一目录里按 `PATHEXT` 的默认次序
///   （`.com` · `.exe` 原生安装器 / winget / scoop · `.bat` · `.cmd` npm / pnpm / yarn）；这几种哪儿都没有时才认 PowerShell 自己的 `.ps1`。
///   **原名不算**：npm 全局目录里那份不带后缀的是给 Git Bash 的 sh 壳，PowerShell 起不了它。
///
/// 平台是入参（不碰 `cfg!`），`is_file` 是注入的（同名的目录不算）：Linux 上的判据能把 Windows 那一形当夹具跑。
pub(crate) fn find_session_command(
    prog: &str,
    path_env: Option<&str>,
    windows: bool,
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> SessionCommand {
    const UNIX: &[(&str, StartBy)] = &[("", StartBy::Itself)];
    const WINDOWS: &[(&str, StartBy)] = &[
        (".com", StartBy::Itself),
        (".exe", StartBy::Itself),
        (".bat", StartBy::Itself),
        (".cmd", StartBy::Itself),
    ];
    const WINDOWS_LAST: &[(&str, StartBy)] = &[(".ps1", StartBy::PowerShellFile)];
    let Some(pe) = path_env.filter(|p| !p.trim().is_empty()) else {
        return SessionCommand::Unknown;
    };
    let dirs = path_dirs(pe);
    let rounds: &[&[(&str, StartBy)]] = if windows {
        &[WINDOWS, WINDOWS_LAST]
    } else {
        &[UNIX]
    };
    for forms in rounds {
        for d in &dirs {
            for &(suffix, by) in *forms {
                let file = d.join(format!("{prog}{suffix}"));
                if !is_file(&file) {
                    continue;
                }
                let (program, lead) = match by {
                    StartBy::Itself => (file.clone().into_os_string(), Vec::new()),
                    StartBy::PowerShellFile => (
                        "powershell.exe".into(),
                        vec![
                            "-NoProfile".into(),
                            "-File".into(),
                            file.clone().into_os_string(),
                        ],
                    ),
                };
                return SessionCommand::Found {
                    file,
                    program,
                    lead,
                };
            }
        }
    }
    SessionCommand::Absent
}

/// `PATH` 切成目录（Windows 的 `;` 与盘符冒号走 `std::env::split_paths`；空项不算）。后端里切 `PATH` 只此一处：
/// 找命令一律经 [`find_session_command`]；别处只许拿它数一数查过几个目录（找不到时那句话里说）。
pub(crate) fn path_dirs(path_env: &str) -> Vec<std::path::PathBuf> {
    std::env::split_paths(path_env)
        .filter(|d| !d.as_os_str().is_empty())
        .collect()
}

/// `<shell> -l -i -c <打出 PATH>`，`TERM=dumb`（不起）。
pub(crate) fn login_shell_asking_path(shell: &str) -> Child {
    Child::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            &format!(
                "printf '%s%s%s' {m} \"$PATH\" {m}",
                m = shell_quote_core::posix_quote(SESSION_PATH_MARK)
            ),
        ])
        .env("TERM", "dumb")
}

/// 起它（stdin 接空 · 自成一组 · 到期限整组杀，都是 [`Child::run`] 的）、从输出里认那一段。
pub(crate) fn ask_session_path(
    cmd: Child,
    within: crate::platform::child::Deadline,
) -> Option<String> {
    match cmd.run(within) {
        Ok(out) => parse_marked_path(&String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!("问起会话那个 shell 的 PATH 没问成：{e}");
            None
        }
    }
}

/// 输出里最后一对标记中间那一段；没有 / 空 ⇒ `None`。
pub(crate) fn parse_marked_path(out: &str) -> Option<String> {
    let (head, _) = out.rsplit_once(SESSION_PATH_MARK)?;
    let (_, path) = head.rsplit_once(SESSION_PATH_MARK)?;
    (!path.trim().is_empty()).then(|| path.to_string())
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
