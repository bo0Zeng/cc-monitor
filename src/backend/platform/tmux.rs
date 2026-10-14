//! 这台的 tmux：**平台**这一维（编译期定死的 [`TMUX_PLATFORM`]）× **探针**那一维（扫 `PATH`）。
//! 「这台机器上做不到哪些命令」（`crate::unavailable_here`）· 起会话命令行的能力（`control::ccm::launcher_with`）·
//! 能力账按 target 现推都从这里问，不另立一份平台知识。

/// `K-P4`：与世界打交道的那一半 —— 给定 `PATH` 的值，`tmux` 在不在它上面。
///
/// # 判准为什么正好是「`PATH` 上有没有一个可执行的 `tmux`」
///
/// 因为**真调用那一刻就是这么找的**：`control/kill.rs` `control/gate.rs` `control/launch.rs`
/// 三处都走 `Command::new("tmux")`，unix 上它是 `execvp` ⇒ 逐字就是 `PATH` 查找。
/// 事前那句话与事后那句话用**同一个判准**，两者才不会各说各话。
///
/// # `None`（判不出来）今天只剩**一个**来源
///
/// ① `PATH` 没设、或是空的 ⇒ **无处可查**，「没找到」这句话说不出口。
///
/// # 🔴 首行那个 `cfg!(unix)` 今天只说一件事（`K-P4` 下一拍拆开的就是它）
///
/// 上一版它**同时**说了两句话：「**我这个探针在非 unix 上不工作**」（**真的** ——
/// Windows 的 `CreateProcess` 还会看进程自身目录、当前目录、`System32`，并按 `PATHEXT`
/// 补后缀，真装了的那个叫 `tmux.exe`，而本扫描找的是无后缀的 `tmux`）
/// 与「**所以答案未知**」（**假的**）。后一句让这张表**在 Windows 上恒空**，
/// 而 Windows 正是这一件的动机平台 —— 原始问题在那儿一格都没被治。
///
/// 现在**平台那一维搬去了 [`TmuxPlatform`]**：非 unix 上压根走不到这个函数
/// （`tmux_present` 在 windows 那一档调的是 [`tmux_exe_in`]）。
/// ⇒ 这一行留着，只表达**它自己那一句**：「扫 `PATH` 找无后缀 `tmux`」这个判准
/// 只在 unix 上与 `execvp` 等价，别处不等价，所以它在别处**不开口**。
pub fn tmux_in(path: Option<&std::ffi::OsStr>) -> Option<bool> {
    if !cfg!(unix) {
        return None;
    }
    tmux_found(path, false, &|p| super::fs::is_executable(p))
}

/// `PATH` 上找 `tmux`：与找 agent 命令行同一处（[`super::shell::find_session_command`]，后端里逐目录找命令只有它）。
/// `PATH` 没设 / 空（或不是 UTF-8）⇒ `None`（无处可查）；`is_file` 定什么样的文件算数。
fn tmux_found(
    path: Option<&std::ffi::OsStr>,
    windows: bool,
    is_file: &dyn Fn(&std::path::Path) -> bool,
) -> Option<bool> {
    match super::shell::find_session_command(
        "tmux",
        path.and_then(std::ffi::OsStr::to_str),
        windows,
        is_file,
    ) {
        super::shell::SessionCommand::Unknown => None,
        super::shell::SessionCommand::Absent => Some(false),
        super::shell::SessionCommand::Found { .. } => Some(true),
    }
}

/// `K-P4`（下一拍）：**平台**这一维 —— 与「探针」那一维分开的第二个值。
///
/// # 它治的是「一个值装了两件事」
///
/// 上一拍 `tmux_in` 的首行把「**探针在这个平台上不工作**」（真）与「**答案未知**」（假）
/// 压成了同一个 `None`。于是 `unavailable_from(None)` 不列任何命令 ⇒
/// **这张表在 Windows 上恒空** ⇒「没有 tmux 却宣称我认 `kill`/`launch`」这个原始问题，
/// 在**动机平台**上一格都没治。⇒ 本枚举把那两件事拆成两个值。
///
/// # 三档，🔴 不许再合并
///
/// ⚠ 拆开的是**平台**这一维，**不是**三态处置那条规则 —— 那条（「不知道」不许压成
/// 「做不到」）在 [`crate::unavailable_from`] 的头注里，这里一个字没动，也不该动：
/// 它论证过「不知道」压成「做不到」会让**能用的功能从界面上无声消失**。
/// 上一拍的病不在那条规则，在**把 Windows 归进了「不知道」这一档**。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TmuxPlatform {
    /// **unix** —— 平台这一维**没有结论**，整件事交给探针（[`tmux_in`] 扫 `PATH`）。
    /// 探针的三档原样保留：找到 = `Some(true)` · 没找到 = `Some(false)` ·
    /// `PATH` 无处可查 = `None`。
    AskThePath,
    /// **windows** —— 那儿**没有原生 tmux**（它要 `fork` / pty / unix domain socket）
    /// ⇒ 默认答案是**确证的「没有」**，这一句在**编译期**就成立，不需要探针去证。
    ///
    /// 探针在这一档里只有**一个方向**的作用：把答案**抬成「有」**（有人把 MSYS2 /
    /// Cygwin 的 `tmux.exe` 放上了 `PATH` —— 那台机器上 `Command::new("tmux")` 真的能起来）；
    /// 它**永远不会**把答案压回「不知道」。
    ///
    /// ⇒ 漏看的方向也是安全的：`tmux_exe_in` 看不见的地方（进程自身目录 / 当前目录 /
    /// `System32`），我们说「没有」，而同一台机器真调用时 `CreateProcess` **也搜 `PATH`**、
    /// 一样失败、一样回 `no_tmux` ⇒ **事前那句话与事后那句话仍然是同一句**。
    AbsentUnlessExeOnPath,
    /// **既不是 unix 也不是 windows** —— 探针不适用，平台这一维**也没有结论**
    /// ⇒ 老实说「不知道」，按三态处置那条不列进表。
    /// 🔴 **这一档不许再拿来装 Windows**：那正是上一拍的病。
    NoOpinion,
}

/// `K-P4`：Windows 形状的探针 —— `PATH` 上有没有一个叫 `tmux.exe` 的普通文件。
///
/// # 为什么不复用 [`tmux_in`]
///
/// 那个找的是**无后缀**的 `tmux`，还要执行位；而 Windows 上真装了的叫 `tmux.exe`，
/// 且那儿没有执行位这个概念（`super::fs::is_executable` 在非 unix 上恒真）。
/// 两条都不是那边的形状 ⇒ 各写各的判准，别让一个函数装两个平台的语义。
///
/// # 它证不到什么（如实写）
///
/// `CreateProcess` 的查找面比 `PATH` 大（进程自身目录 · 当前目录 · `System32` · `Windows`），
/// 还会按 `PATHEXT` 补后缀 ⇒ **它可能漏看**。漏看之后答案是「没有」，
/// 而那正是 [`TmuxPlatform::AbsentUnlessExeOnPath`] 头注里论证过的安全方向。
pub fn tmux_exe_in(path: Option<&std::ffi::OsStr>) -> bool {
    // 只认 `tmux.exe`（起 tmux 那一下 `CreateProcess` 只补 `.exe`）：别的后缀那几形（`.cmd` · `.ps1` …）不算数。
    let exe = |p: &std::path::Path| {
        p.file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("tmux.exe"))
            && p.is_file()
    };
    tmux_found(path, true, &exe) == Some(true)
}

/// `K-P4`：两维合起来 —— **平台**（编译期定死）× **探针**（去世界上看）。
///
/// 三个入参组合的答案由 [`TmuxPlatform`] 各档头注给；这个函数本身**不碰 `cfg!`**，
/// 平台是**入参**。⇒ 本机是 Linux 也能把「Windows 那台机器」当成一个入参跑出来，
/// 而那正是 `the_windows_answer_is_confirmed_absent_not_unknown` 拿到读数的方式。
pub fn tmux_present(platform: TmuxPlatform, path: Option<&std::ffi::OsStr>) -> Option<bool> {
    match platform {
        TmuxPlatform::AskThePath => tmux_in(path),
        TmuxPlatform::AbsentUnlessExeOnPath => Some(tmux_exe_in(path)),
        TmuxPlatform::NoOpinion => None,
    }
}

/// 这份二进制编译到哪个平台 —— **全文件唯一**一处把平台翻成值的地方。
///
/// 「唯一一处」与「windows 那一支给的是哪一档」由
/// `the_windows_arm_is_wired_into_the_source` 钉住（它读的是**磁盘上的源码文本**）。
pub const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {
    TmuxPlatform::AbsentUnlessExeOnPath
} else if cfg!(unix) {
    TmuxPlatform::AskThePath
} else {
    TmuxPlatform::NoOpinion
};

/// ★ 一条**只在 Windows 编译时存在**的编译期断言。
///
/// 🔴 **它在本仓门禁上给不出任何读数** —— 本机是 Linux，这个 item 在这儿
/// **编译期就不存在**。它开口的时刻是任何一次 **Windows 编译**
/// （backend「必须在 Windows 上编得过」这条纪律见 `platform/fs::is_executable` 头注）。
/// 写它的理由：Linux 那侧只有源码文本判据（读的是「那一支写在那儿」），
/// 而这一条读的是「**那一支真的被编进去了**」—— 两者证的不是同一件事。
#[cfg(windows)]
const _: () = assert!(
    matches!(TMUX_PLATFORM, TmuxPlatform::AbsentUnlessExeOnPath),
    "Windows 上平台这一维必须是「确证没有」而不是「不知道」——\
     改回 NoOpinion 会让握手帧第四条面在 Windows 上恒空，而 Windows 正是这一件的动机平台。"
);
